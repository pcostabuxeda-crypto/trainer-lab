//! Motor de trucs: aplica les accions d'una definició sobre el joc adjuntat.
//!
//! Per a cada truc guardem el punt de partida de l'adreça (resultat de l'AOB, car de
//! calcular) i tornem a recórrer la cadena de punters a cada ús. Si un truc activat ha
//! modificat la memòria, guardem els bytes originals per poder-los restaurar.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{CheatDef, CheatKind, ToggleOff, TrainerDef};
use crate::error::{AppError, AppResult};
use crate::memory::freeze::Freezer;
use crate::memory::{locator, pointer};
use crate::memory::value::{ValueType, parse_hex_bytes};
use crate::memory::{Memory, ModuleInfo, WriteMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VersionStatus {
    /// L'SHA-256 de l'executable coincideix amb una versió de la definició
    Verified,
    /// La definició llista versions però cap coincideix
    Unverified,
    /// La definició no llista cap versió
    Unknown,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VersionInfo {
    pub status: VersionStatus,
    pub label: Option<String>,
    pub sha256: String,
}

impl VersionInfo {
    pub fn check(def: &TrainerDef, sha256: String) -> Self {
        let found = def.versions.iter().find(|v| v.sha256.eq_ignore_ascii_case(&sha256));
        let status = match (found, def.versions.is_empty()) {
            (Some(_), _) => VersionStatus::Verified,
            (None, true) => VersionStatus::Unknown,
            (None, false) => VersionStatus::Unverified,
        };
        Self { status, label: found.map(|v| v.label.clone()), sha256 }
    }
}

/// Què vol fer l'usuari amb un truc.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum CheatInput {
    /// toggle
    Toggle { on: bool },
    /// slider, value
    Set { value: f64 },
    /// slider: un `step` amunt o avall (hotkeys)
    Step { up: bool },
    /// button
    Press,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheatState {
    pub id: String,
    pub active: bool,
    /// Valor actual a la memòria (no per als patches)
    pub value: Option<f64>,
    /// Adreça actual, en hex
    pub address: Option<String>,
    pub error: Option<String>,
}

/// Buscar pels noms IL2CPP costa uns segons: mentre el joc no és a punt, no a cada refresc.
const IL2CPP_RETRY: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
struct Runtime {
    /// Punt de partida i offsets de la cadena
    start: Option<(u64, Vec<i64>)>,
    /// Última vegada que s'ha intentat localitzar sense èxit
    failed_at: Option<Instant>,
    active: bool,
    original: Option<Vec<u8>>,
    value: Option<f64>,
    address: Option<u64>,
    error: Option<String>,
}

pub struct ActiveTrainer {
    pub def: TrainerDef,
    pub version: VersionInfo,
    runtime: BTreeMap<String, Runtime>,
}

impl ActiveTrainer {
    /// Resol el punt de partida de cada truc. Els que fallen queden amb l'error i es
    /// tornen a provar quan l'usuari els fa servir.
    pub fn new(def: TrainerDef, version: VersionInfo, mem: &dyn Memory, modules: &[ModuleInfo]) -> Self {
        // Els trucs amb la mateixa adreça només es busquen un cop
        let mut known = BTreeMap::new();
        let runtime = def
            .cheats
            .iter()
            .map(|c| {
                let chain = known
                    .entry(format!("{:?}", c.target.address))
                    .or_insert_with(|| locator::resolve_chain(mem, modules, &c.target.address).map_err(|e| e.to_string()));
                let rt = match chain {
                    Ok(chain) => Runtime { start: Some(chain.clone()), ..Default::default() },
                    Err(e) => {
                        tracing::warn!(cheat = %c.id, "no s'ha pogut localitzar: {e}");
                        Runtime { error: Some(e.clone()), failed_at: Some(Instant::now()), ..Default::default() }
                    }
                };
                (c.id.clone(), rt)
            })
            .collect();
        Self { def, version, runtime }
    }

    pub fn cheat(&self, id: &str) -> AppResult<&CheatDef> {
        self.def.cheats.iter().find(|c| c.id == id).ok_or_else(|| AppError::InvalidValue(format!("no hi ha cap truc «{id}»")))
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.runtime.get(id).is_some_and(|r| r.active)
    }

    pub fn state(&self, id: &str) -> CheatState {
        let rt = self.runtime.get(id);
        CheatState {
            id: id.into(),
            active: rt.is_some_and(|r| r.active),
            value: rt.and_then(|r| r.value),
            address: rt.and_then(|r| r.address).map(|a| format!("{a:#x}")),
            error: rt.and_then(|r| r.error.clone()),
        }
    }

    /// Adreça actual del truc: punt de partida (guardat) + cadena de punters.
    fn address(&mut self, cheat: &CheatDef, mem: &dyn Memory, modules: &[ModuleInfo]) -> AppResult<u64> {
        let rt = self.runtime.entry(cheat.id.clone()).or_default();
        if rt.start.is_none() {
            rt.failed_at = Some(Instant::now());
            rt.start = Some(locator::resolve_chain(mem, modules, &cheat.target.address)?);
        }
        let Some((start, offsets)) = &rt.start else { unreachable!() };
        let addr = pointer::resolve(mem, *start, offsets)?;
        rt.address = Some(addr);
        Ok(addr)
    }

    /// Congela el valor del truc seguint la seva cadena de punters.
    fn freeze(&self, c: &CheatDef, addr: u64, bytes: Vec<u8>, freezer: &Freezer) {
        match self.runtime.get(&c.id).and_then(|r| r.start.as_ref()) {
            Some((start, offsets)) => freezer.set_chain(&c.id, *start, offsets.clone(), addr, bytes),
            None => freezer.set(&c.id, addr, bytes),
        }
    }

    pub fn apply(
        &mut self,
        id: &str,
        input: CheatInput,
        mem: &dyn Memory,
        modules: &[ModuleInfo],
        freezer: &Freezer,
    ) -> AppResult<CheatState> {
        let cheat = self.cheat(id)?.clone();
        let result = self.apply_inner(&cheat, input, mem, modules, freezer);
        let rt = self.runtime.entry(cheat.id.clone()).or_default();
        rt.error = result.as_ref().err().map(|e| e.to_string());
        match &result {
            Ok(()) => tracing::info!(cheat = %cheat.id, input = ?input, "truc aplicat"),
            Err(e) => tracing::warn!(cheat = %cheat.id, input = ?input, "el truc ha fallat: {e}"),
        }
        result?;
        if cheat.target.value_type != ValueType::Bytes {
            let _ = self.read_value(&cheat, mem, modules);
        }
        Ok(self.state(&cheat.id))
    }

    fn apply_inner(
        &mut self,
        c: &CheatDef,
        input: CheatInput,
        mem: &dyn Memory,
        modules: &[ModuleInfo],
        freezer: &Freezer,
    ) -> AppResult<()> {
        let ty = c.target.value_type;
        let wrong = || Err(AppError::InvalidValue(format!("l'acció {input:?} no és vàlida per a un truc {:?}", c.kind)));
        match (c.kind, input) {
            (CheatKind::Toggle, CheatInput::Toggle { on: true }) => {
                if self.is_active(&c.id) {
                    return Ok(());
                }
                let on = c.on.clone().unwrap_or_default();
                let addr = self.address(c, mem, modules)?;
                let original = if let Some(patch) = &on.patch {
                    let patch = parse_hex_bytes(patch)?;
                    let expect = parse_hex_bytes(on.expect.as_deref().unwrap_or_default())?;
                    let mut current = vec![0u8; patch.len()];
                    mem.read_bytes(addr, &mut current)?;
                    if current != expect && current != patch {
                        return Err(AppError::WriteRejected {
                            address: addr,
                            reason: "els bytes no coincideixen amb `expect` (potser és una altra versió del joc)".into(),
                        });
                    }
                    mem.write_bytes(addr, &patch, WriteMode::Code)?;
                    expect
                } else {
                    let v = on.freeze.or(on.write).unwrap_or_default();
                    let bytes = ty.encode(v)?;
                    let mut original = vec![0u8; bytes.len()];
                    mem.read_bytes(addr, &mut original)?;
                    mem.write_bytes(addr, &bytes, WriteMode::Data)?;
                    if on.freeze.is_some() {
                        self.freeze(c, addr, bytes, freezer);
                    }
                    original
                };
                let rt = self.runtime.entry(c.id.clone()).or_default();
                rt.active = true;
                rt.original = Some(original);
                Ok(())
            }
            (CheatKind::Toggle, CheatInput::Toggle { on: false }) => self.deactivate(c, mem, freezer),
            (CheatKind::Slider | CheatKind::Value, CheatInput::Set { value }) => {
                if c.min.is_some_and(|m| value < m) || c.max.is_some_and(|m| value > m) {
                    return Err(AppError::InvalidValue(format!(
                        "{value} està fora del rang {}..={}",
                        c.min.map_or("-∞".into(), |m| m.to_string()),
                        c.max.map_or("∞".into(), |m| m.to_string())
                    )));
                }
                let bytes = ty.encode(value)?;
                let addr = self.address(c, mem, modules)?;
                mem.write_bytes(addr, &bytes, WriteMode::Data)?;
                if c.kind == CheatKind::Slider && c.freeze == Some(true) {
                    self.freeze(c, addr, bytes, freezer);
                    self.runtime.entry(c.id.clone()).or_default().active = true;
                }
                Ok(())
            }
            (CheatKind::Slider, CheatInput::Step { up }) => {
                let current = self.read_value(c, mem, modules)?;
                let step = c.step.unwrap_or(1.0);
                let next = if up { current + step } else { current - step };
                let next = next.clamp(c.min.unwrap_or(f64::MIN), c.max.unwrap_or(f64::MAX));
                self.apply_inner(c, CheatInput::Set { value: next }, mem, modules, freezer)
            }
            (CheatKind::Button, CheatInput::Press) => {
                let action = c.action.clone().unwrap_or_default();
                let value = match (action.add, action.set) {
                    (Some(add), _) => self.read_value(c, mem, modules)? + add,
                    (None, Some(set)) => set,
                    _ => return wrong(),
                };
                let bytes = ty.encode(value)?;
                let addr = self.address(c, mem, modules)?;
                mem.write_bytes(addr, &bytes, WriteMode::Data)
            }
            _ => wrong(),
        }
    }

    fn deactivate(&mut self, c: &CheatDef, mem: &dyn Memory, freezer: &Freezer) -> AppResult<()> {
        freezer.remove(&c.id);
        let rt = self.runtime.entry(c.id.clone()).or_default();
        if !rt.active {
            return Ok(());
        }
        rt.active = false;
        let original = rt.original.take();
        let addr = rt.address;
        if c.off.unwrap_or_default() == ToggleOff::Restore
            && let (Some(bytes), Some(addr)) = (original, addr)
        {
            let mode = if c.target.value_type == ValueType::Bytes { WriteMode::Code } else { WriteMode::Data };
            mem.write_bytes(addr, &bytes, mode)?;
        }
        Ok(())
    }

    fn read_value(&mut self, c: &CheatDef, mem: &dyn Memory, modules: &[ModuleInfo]) -> AppResult<f64> {
        let ty = c.target.value_type;
        let addr = self.address(c, mem, modules)?;
        let mut buf = vec![0u8; ty.size()];
        mem.read_bytes(addr, &mut buf)?;
        let v = ty.decode(&buf)?;
        self.runtime.entry(c.id.clone()).or_default().value = Some(v);
        Ok(v)
    }

    /// Llegeix el valor actual de tots els trucs (per a la UI).
    pub fn refresh(&mut self, mem: &dyn Memory, modules: &[ModuleInfo]) -> Vec<CheatState> {
        for c in self.def.cheats.clone() {
            if c.target.value_type == ValueType::Bytes {
                continue;
            }
            let rt = self.runtime.entry(c.id.clone()).or_default();
            if c.target.address.il2cpp.is_some() && rt.start.is_none() && rt.failed_at.is_some_and(|t| t.elapsed() < IL2CPP_RETRY) {
                continue;
            }
            let read = self.read_value(&c, mem, modules);
            let rt = self.runtime.entry(c.id.clone()).or_default();
            match read {
                // L'error d'una acció anterior es manté; el d'una lectura fallida, no
                Ok(_) if rt.error.as_deref().is_some_and(|e| e.starts_with("No s'ha pogut llegir") || e.starts_with("Punter nul") || e.starts_with("Encara no")) => {
                    rt.error = None
                }
                Ok(_) => {}
                Err(e) => {
                    rt.value = None;
                    rt.error = Some(e.to_string());
                }
            }
        }
        self.def.cheats.iter().map(|c| self.state(&c.id)).collect()
    }

    /// Desactiva tots els toggles (restaurant la memòria). Es fa en desadjuntar.
    pub fn deactivate_all(&mut self, mem: &dyn Memory, freezer: &Freezer) {
        for c in self.def.cheats.clone() {
            freezer.remove(&c.id);
            if c.kind == CheatKind::Toggle
                && let Err(e) = self.deactivate(&c, mem, freezer)
            {
                tracing::warn!(cheat = %c.id, "no s'ha pogut restaurar: {e}");
            }
            if let Some(rt) = self.runtime.get_mut(&c.id) {
                rt.active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::memory::fake::FakeMemory;

    // "Joc" simulat: un mòdul a 0x40000; a +0x100 un punter al jugador (0x40180).
    // Jugador: vida i32 a +0, munició i32 a +4. A +0x20, codi "FF 4B 10" (dec [rbx+10]).
    fn setup() -> (Arc<FakeMemory>, Vec<ModuleInfo>, ActiveTrainer, Freezer) {
        let m = Arc::new(FakeMemory::new(0x40000, 0x200, 8));
        m.put_ptr(0x40100, 0x40180);
        m.put(0x40180, &100i32.to_le_bytes());
        m.put(0x40184, &30i32.to_le_bytes());
        m.put(0x40020, &[0xFF, 0x4B, 0x10]);
        let mods = vec![ModuleInfo { name: "joc.exe".into(), base: 0x40000, size: 0x200 }];
        let at = |f: &str| format!(r#"{{ "offset": "0x100", "pointers": ["{f}"] }}"#);
        let json = format!(
            r#"{{ "schemaVersion": 1, "id": "joc", "name": "Joc", "process": {{ "executable": "joc.exe" }}, "cheats": [
              {{ "id": "vida", "name": "Vida", "kind": "toggle", "target": {{ "address": {}, "type": "i32" }}, "on": {{ "freeze": 999 }} }},
              {{ "id": "municio", "name": "Munició", "kind": "slider", "target": {{ "address": {}, "type": "i32" }}, "min": 0, "max": 50, "step": 10 }},
              {{ "id": "mes", "name": "+5", "kind": "button", "target": {{ "address": {}, "type": "i32" }}, "action": {{ "add": 5 }} }},
              {{ "id": "nop", "name": "Nop", "kind": "toggle", "target": {{ "address": {{ "offset": "0x20" }}, "type": "bytes" }},
                 "on": {{ "patch": "90 90 90", "expect": "FF 4B 10" }} }}
            ] }}"#,
            at("0x0"),
            at("0x4"),
            at("0x4")
        );
        let def: TrainerDef = serde_json::from_str(&json).unwrap();
        crate::trainers::model::validate(&def).unwrap();
        let t = ActiveTrainer::new(def, VersionInfo { status: VersionStatus::Unknown, label: None, sha256: String::new() }, m.as_ref(), &mods);
        let f = Freezer::start(m.clone(), 10);
        (m, mods, t, f)
    }

    fn i32_at(m: &FakeMemory, a: u64) -> i32 {
        let mut b = [0u8; 4];
        m.read_bytes(a, &mut b).unwrap();
        i32::from_le_bytes(b)
    }

    #[test]
    fn toggle_freeze_and_restore() {
        let (m, mods, mut t, f) = setup();
        let s = t.apply("vida", CheatInput::Toggle { on: true }, m.as_ref(), &mods, &f).unwrap();
        assert!(s.active);
        assert_eq!(s.value, Some(999.0));
        m.put(0x40180, &1i32.to_le_bytes());
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(i32_at(&m, 0x40180), 999, "congelat");
        t.apply("vida", CheatInput::Toggle { on: false }, m.as_ref(), &mods, &f).unwrap();
        assert_eq!(i32_at(&m, 0x40180), 100, "restaura el valor original");
    }

    #[test]
    fn slider_range_step_and_button() {
        let (m, mods, mut t, f) = setup();
        t.apply("municio", CheatInput::Set { value: 40.0 }, m.as_ref(), &mods, &f).unwrap();
        assert_eq!(i32_at(&m, 0x40184), 40);
        assert!(t.apply("municio", CheatInput::Set { value: 51.0 }, m.as_ref(), &mods, &f).is_err(), "fora de rang");
        t.apply("municio", CheatInput::Step { up: true }, m.as_ref(), &mods, &f).unwrap();
        assert_eq!(i32_at(&m, 0x40184), 50, "puja un step i limita al màxim");
        t.apply("municio", CheatInput::Step { up: false }, m.as_ref(), &mods, &f).unwrap();
        assert_eq!(i32_at(&m, 0x40184), 40);
        t.apply("mes", CheatInput::Press, m.as_ref(), &mods, &f).unwrap();
        assert_eq!(i32_at(&m, 0x40184), 45);
        assert!(t.apply("mes", CheatInput::Toggle { on: true }, m.as_ref(), &mods, &f).is_err(), "acció incorrecta");
    }

    #[test]
    fn patch_checks_expected_bytes_and_restores() {
        let (m, mods, mut t, f) = setup();
        t.apply("nop", CheatInput::Toggle { on: true }, m.as_ref(), &mods, &f).unwrap();
        let mut b = [0u8; 3];
        m.read_bytes(0x40020, &mut b).unwrap();
        assert_eq!(b, [0x90, 0x90, 0x90]);
        t.deactivate_all(m.as_ref(), &f);
        m.read_bytes(0x40020, &mut b).unwrap();
        assert_eq!(b, [0xFF, 0x4B, 0x10], "desadjuntar restaura el codi");

        m.put(0x40020, &[0x11, 0x22, 0x33]); // "una altra versió del joc"
        let e = t.apply("nop", CheatInput::Toggle { on: true }, m.as_ref(), &mods, &f).unwrap_err();
        assert!(e.to_string().contains("expect"));
        m.read_bytes(0x40020, &mut b).unwrap();
        assert_eq!(b, [0x11, 0x22, 0x33], "no ha escrit res");
        assert!(t.state("nop").error.is_some());
    }

    #[test]
    fn freeze_follows_pointer_changes() {
        let (m, mods, mut t, f) = setup();
        t.apply("vida", CheatInput::Toggle { on: true }, m.as_ref(), &mods, &f).unwrap();
        // El joc "carrega partida": el jugador es mou a 0x401C0
        m.put(0x401C0, &50i32.to_le_bytes());
        m.put_ptr(0x40100, 0x401C0);
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(i32_at(&m, 0x401C0), 999, "congela la nova adreça sense ajuda de la UI");
        let states = t.refresh(m.as_ref(), &mods);
        assert_eq!(states[0].address.as_deref(), Some("0x401c0"));
        assert_eq!(states[0].value, Some(999.0));
    }

    #[test]
    fn version_status() {
        let (_, _, t, _) = setup();
        let mut def = t.def.clone();
        assert_eq!(VersionInfo::check(&def, "aa".into()).status, VersionStatus::Unknown);
        def.versions.push(crate::trainers::model::GameVersion { label: "1.0".into(), sha256: "AB".repeat(32) });
        assert_eq!(VersionInfo::check(&def, "ab".repeat(32)).status, VersionStatus::Verified);
        assert_eq!(VersionInfo::check(&def, "cd".repeat(32)).status, VersionStatus::Unverified);
    }
}
