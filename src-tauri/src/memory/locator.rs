//! Com trobar l'adreça d'un valor: un punt de partida (mòdul + offset, o un patró AOB)
//! i, opcionalment, una cadena de punters. És el camp `address` de les definicions de
//! trainers (vegeu docs/PLA_ARQUITECTURA.md §4.2).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use super::{Memory, ModuleInfo, aob, pointer};
use crate::error::{AppError, AppResult};

/// Offset amb signe. En JSON s'accepta un número (`16`) o un text (`"0x10"`, `"-0x8"`, `"16"`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, TS)]
#[ts(export, type = "string | number")]
pub struct Offset(pub i64);

impl Offset {
    pub fn parse(s: &str) -> Option<i64> {
        let s = s.trim();
        let (neg, s) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
        let v = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(hex) => i64::from_str_radix(hex, 16).ok()?,
            None => s.parse().ok()?,
        };
        Some(if neg { -v } else { v })
    }
}

impl<'de> Deserialize<'de> for Offset {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(i64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Num(n) => Ok(Offset(n)),
            Raw::Text(t) => Offset::parse(&t)
                .map(Offset)
                .ok_or_else(|| serde::de::Error::custom(format!("offset invàlid «{t}» (ex.: \"0x10\" o 16)"))),
        }
    }
}

impl Serialize for Offset {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 0 { write!(f, "-{:#x}", self.0.unsigned_abs()) } else { write!(f, "{:#x}", self.0) }
    }
}

/// Instrucció x64 amb adreçament relatiu a RIP, p. ex. `48 8B 05 <disp32>` = `mov rax, [rip+disp]`.
/// L'adreça final és: inici de la instrucció + `instrLength` + disp32 llegit a `dispOffset`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct Rip {
    pub disp_offset: u32,
    pub instr_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AobLocator {
    pub pattern: String,
    /// Es suma a la coincidència (o al resultat de `rip`)
    #[serde(default)]
    pub offset: Offset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rip: Option<Rip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct Locator {
    /// Nom del mòdul (`game.exe`, `engine.dll`). Si falta, l'executable principal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub offset: Option<Offset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub aob: Option<AobLocator>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pointers: Vec<Offset>,
}

pub fn find_module<'a>(modules: &'a [ModuleInfo], name: Option<&str>) -> AppResult<&'a ModuleInfo> {
    match name {
        // Toolhelp retorna sempre l'executable principal com a primer mòdul
        None => modules.first().ok_or_else(|| AppError::ModuleNotFound("(executable principal)".into())),
        Some(n) => modules
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(n))
            .ok_or_else(|| AppError::ModuleNotFound(n.into())),
    }
}

pub fn resolve(mem: &dyn Memory, modules: &[ModuleInfo], loc: &Locator) -> AppResult<u64> {
    let module = find_module(modules, loc.module.as_deref())?;
    let start = match (&loc.offset, &loc.aob) {
        (Some(off), None) => module.base.wrapping_add_signed(off.0),
        (None, Some(a)) => {
            let pattern: aob::Pattern = a.pattern.parse()?;
            let hits = aob::scan(mem, &pattern, module.base, module.base + module.size, 2);
            let Some(&hit) = hits.first() else { return Err(AppError::PatternNotFound(module.name.clone())) };
            if hits.len() > 1 {
                tracing::warn!(pattern = %a.pattern, "el patró AOB apareix més d'un cop; es fa servir el primer");
            }
            let base = match &a.rip {
                Some(rip) => {
                    let mut d = [0u8; 4];
                    mem.read_bytes(hit + rip.disp_offset as u64, &mut d)?;
                    (hit + rip.instr_length as u64).wrapping_add_signed(i32::from_le_bytes(d) as i64)
                }
                None => hit,
            };
            base.wrapping_add_signed(a.offset.0)
        }
        _ => return Err(AppError::InvalidValue("l'adreça necessita exactament un de: `offset` o `aob`".into())),
    };
    let offsets: Vec<i64> = loc.pointers.iter().map(|o| o.0).collect();
    pointer::resolve(mem, start, &offsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::fake::FakeMemory;

    // Un "mòdul" de 0x200 bytes a 0x40000 amb:
    // - a +0x100 el magic TLABANCH seguit d'un punter al "jugador" (0x40180)
    // - a +0x20 una instrucció `48 8B 05 disp32` que apunta a +0x100 (rip-relative)
    fn setup() -> (FakeMemory, Vec<ModuleInfo>) {
        let m = FakeMemory::new(0x40000, 0x200, 8);
        m.put(0x40100, b"TLABANCH");
        m.put_ptr(0x40108, 0x40180);
        m.put(0x40188, &500i64.to_le_bytes());
        let disp = (0x40100i64 - (0x40020 + 7)) as i32;
        m.put(0x40020, &[0x48, 0x8B, 0x05]);
        m.put(0x40023, &disp.to_le_bytes());
        m.put(0x40027, &[0x48, 0x85, 0xC0]); // test rax, rax
        (m, vec![ModuleInfo { name: "Game.exe".into(), base: 0x40000, size: 0x200 }])
    }

    fn loc(json: &str) -> Locator {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn module_offset_plus_pointers() {
        let (m, mods) = setup();
        let l = loc(r#"{ "module": "game.exe", "offset": "0x108", "pointers": ["0x8"] }"#);
        assert_eq!(resolve(&m, &mods, &l).unwrap(), 0x40188);
    }

    #[test]
    fn aob_with_offset_and_pointers() {
        let (m, mods) = setup();
        let l = loc(r#"{ "aob": { "pattern": "54 4C 41 42 41 4E 43 48", "offset": 8 }, "pointers": ["0x8"] }"#);
        assert_eq!(resolve(&m, &mods, &l).unwrap(), 0x40188);
    }

    #[test]
    fn aob_rip_relative() {
        let (m, mods) = setup();
        let l = loc(r#"{ "aob": { "pattern": "48 8B 05 ?? ?? ?? ?? 48 85 C0", "rip": { "dispOffset": 3, "instrLength": 7 } } }"#);
        assert_eq!(resolve(&m, &mods, &l).unwrap(), 0x40100);
    }

    #[test]
    fn errors_are_explicit() {
        let (m, mods) = setup();
        let both = loc(r#"{ "offset": 0, "aob": { "pattern": "01 02 03 04" } }"#);
        assert!(matches!(resolve(&m, &mods, &both), Err(AppError::InvalidValue(_))));
        assert!(matches!(resolve(&m, &mods, &loc(r#"{ "module": "x.dll", "offset": 0 }"#)), Err(AppError::ModuleNotFound(_))));
        assert!(matches!(resolve(&m, &mods, &loc(r#"{ "aob": { "pattern": "01 02 03 04" } }"#)), Err(AppError::PatternNotFound(_))));
        assert!(serde_json::from_str::<Locator>(r#"{ "offset": "zz" }"#).is_err());
        assert!(serde_json::from_str::<Locator>(r#"{ "ofset": 1 }"#).is_err(), "camps desconeguts");
    }

    #[test]
    fn offsets_parse_and_print() {
        assert_eq!(Offset::parse("0x1F"), Some(31));
        assert_eq!(Offset::parse("-0x10"), Some(-16));
        assert_eq!(Offset::parse("42"), Some(42));
        assert_eq!(Offset::parse("0xZZ"), None);
        assert_eq!(Offset(-16).to_string(), "-0x10");
    }
}
