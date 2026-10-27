//! Escàner de memòria: troba on guarda el joc un valor que veus a la pantalla.
//!
//! Com es fa servir:
//! 1. **Primera cerca** amb el valor exacte (p. ex. 100 de vida): surten milers d'adreces.
//! 2. Canvia el valor al joc (rep mal) i fes una **cerca següent**: «exacte 85», «ha
//!    disminuït»… Només queden les adreces que compleixen la condició.
//! 3. Repeteix fins que en quedin poques. Aquella és l'adreça de la vida.
//!
//! Només es miren les regions **escrivibles** (on el joc guarda dades) i les adreces
//! alineades a la mida del tipus (com fan els compiladors), que és molt més ràpid.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::error::{AppError, AppResult};

use super::value::ValueType;
use super::{Memory, ModuleInfo, Region};

/// Bytes que es llegeixen de cop a la primera cerca.
const CHUNK: usize = 4 << 20;
/// Bloc màxim per rellegir resultats propers a les cerques següents.
const BLOCK: u64 = 64 << 10;
/// Límit total de candidats, compartit entre tots els tipus.
pub const MAX_HITS: usize = 2_000_000;
/// Espai d'adreces d'usuari a Windows x64.
const USER_END: u64 = 0x7FFF_FFFF_0000;

#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum ScanFilter {
    Exact { value: f64 },
    Changed,
    Unchanged,
    Increased,
    Decreased,
}

#[derive(Debug, Clone, Copy)]
struct Hit {
    address: u64,
    value_type: ValueType,
    /// Quantes unitats de memòria fan una unitat de la pantalla: 1, o 100 si el joc guarda
    /// cèntims en un enter (47,70 a la pantalla = 4770 a la memòria)
    scale: f64,
    /// Valor a la cerca anterior, en unitats de la pantalla
    previous: f64,
}

pub struct ScanState {
    hits: Vec<Hit>,
    pub truncated: bool,
    pub scans: u32,
    pub read_failures: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanSummary {
    #[ts(type = "number")]
    pub count: usize,
    /// Hi havia més resultats dels que es poden guardar: cal refinar
    pub truncated: bool,
    pub scans: u32,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    /// Candidats que no s'han pogut rellegir a l'última cerca següent.
    #[ts(type = "number")]
    pub read_failures: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanHit {
    pub address: String,
    pub value_type: ValueType,
    /// 1, o 100 si a la memòria hi ha cèntims. `value` i `previous` ja són en unitats de la
    /// pantalla; per escriure, cal multiplicar per `scale`.
    pub scale: f64,
    /// `mòdul+offset` si l'adreça és dins d'un mòdul (estàtica); `None` si és dinàmica (heap)
    pub module_offset: Option<String>,
    pub value: Option<f64>,
    pub previous: f64,
}

fn alignment(ty: ValueType) -> usize {
    ty.size().clamp(1, 4)
}

/// Igualtat "humana": els decimals amb una petita tolerància (100.0001 és 100); els enters
/// en cèntims, a mig cèntim.
fn equals_scaled(ty: ValueType, scale: f64, a: f64, b: f64) -> bool {
    match ty {
        ValueType::F32 | ValueType::F64 => (a - b).abs() < 0.01,
        _ if scale != 1.0 => (a - b).abs() < 0.5 / scale,
        _ => a == b,
    }
}

/// `now` i `before` en unitats de la pantalla.
fn keep(ty: ValueType, scale: f64, filter: ScanFilter, now: f64, before: f64) -> bool {
    let eq = |a, b| equals_scaled(ty, scale, a, b);
    match filter {
        ScanFilter::Exact { value } => eq(now, value),
        ScanFilter::Changed => !eq(now, before),
        ScanFilter::Unchanged => eq(now, before),
        ScanFilter::Increased => now > before && !eq(now, before),
        ScanFilter::Decreased => now < before && !eq(now, before),
    }
}

/// Memòria on un joc guarda les seves dades: privada (heap) o de les seves imatges (globals).
/// Les regions mapejades (fitxers, memòria compartida amb la GPU) es salten, com fa Cheat Engine
/// per defecte: hi ha molts megabytes i gairebé mai hi ha valors del joc.
fn scannable(r: &Region) -> bool {
    r.committed && r.readable && r.writable && !r.guard && !r.mapped
}

pub fn first_scan(mem: &dyn Memory, value_type: ValueType, value: f64) -> ScanState {
    first_scan_in(mem, value_type, value, 0x10000, USER_END, MAX_HITS)
}

/// Un número visible no determina el tipus amb què el joc el desa.
const AUTO_TYPES: [ValueType; 4] = [ValueType::F64, ValueType::F32, ValueType::I64, ValueType::I32];

pub fn first_scan_auto(mem: &dyn Memory, value: f64) -> ScanState {
    first_scan_targets_in(mem, &auto_targets(value), value, 0x10000, USER_END, MAX_HITS)
}

/// Tipus a provar per a un número vist a la pantalla. Si té decimals (47,70), molts jocs
/// el guarden com a enter en cèntims (4770): també es busca així.
fn auto_targets(value: f64) -> Vec<(ValueType, f64)> {
    let mut t: Vec<(ValueType, f64)> = AUTO_TYPES.iter().map(|ty| (*ty, 1.0)).collect();
    let cents = value * 100.0;
    if value.fract() != 0.0 && (cents - cents.round()).abs() < 1e-6 {
        t.extend([(ValueType::I64, 100.0), (ValueType::I32, 100.0)]);
    }
    t
}

/// Comparador ràpid per a la primera cerca: els enters es comparen com a bytes (una
/// comparació de memòria); els decimals es converteixen directament, sense passar per `decode`.
type Matcher = Box<dyn Fn(&[u8]) -> bool>;

fn matcher(ty: ValueType, value: f64) -> Matcher {
    match ty {
        ValueType::F32 => Box::new(move |b| (f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64 - value).abs() < 0.01),
        ValueType::F64 => Box::new(move |b| (f64::from_le_bytes(b[..8].try_into().unwrap()) - value).abs() < 0.01),
        _ => match ty.encode(value) {
            Ok(target) => Box::new(move |b| b == target.as_slice()),
            Err(_) => Box::new(|_| false),
        },
    }
}

fn first_scan_in(mem: &dyn Memory, ty: ValueType, value: f64, start: u64, end: u64, max: usize) -> ScanState {
    first_scan_types_in(mem, &[ty], value, start, end, max)
}

fn first_scan_types_in(mem: &dyn Memory, types: &[ValueType], value: f64, start: u64, end: u64, max: usize) -> ScanState {
    let targets: Vec<(ValueType, f64)> = types.iter().map(|ty| (*ty, 1.0)).collect();
    first_scan_targets_in(mem, &targets, value, start, end, max)
}

/// `targets`: (tipus, escala). La memòria es compara amb `value × escala`.
fn first_scan_targets_in(mem: &dyn Memory, targets: &[(ValueType, f64)], value: f64, start: u64, end: u64, max: usize) -> ScanState {
    let matchers: Vec<_> = targets.iter().copied()
        .map(|(ty, scale)| (ty, scale, if scale == 1.0 { value } else { (value * scale).round() }))
        .filter(|(ty, _, raw)| ty.is_numeric() && ty.encode(*raw).is_ok())
        .map(|(ty, scale, raw)| (ty, scale, matcher(ty, raw))).collect();
    let mut hits = Vec::new();
    let mut truncated = false;
    'regions: for r in mem.regions(start, end).iter().filter(|r| scannable(r)) {
        let mut pos = r.base.max(start);
        let to = r.end().min(end);
        while pos < to {
            let stride = (CHUNK as u64).min(to - pos) as usize;
            // Solapament per als f64/i64 alineats a 4 bytes que travessen un bloc.
            let len = ((stride + 4) as u64).min(to - pos) as usize;
            let mut buf = vec![0u8; len];
            if mem.read_bytes(pos, &mut buf).is_ok() {
                // Una sola lectura de memòria per bloc, compartida entre els tipus.
                for (ty, scale, is_match) in &matchers {
                    let (size, align) = (ty.size(), alignment(*ty));
                    let mut i = ((align as u64 - pos % align as u64) % align as u64) as usize;
                    while i < stride && i + size <= len {
                        let bytes = &buf[i..i + size];
                        if is_match(bytes) {
                            if hits.len() >= max {
                                truncated = true;
                                break 'regions;
                            }
                            let previous = ty.decode(bytes).map(|v| v / scale).unwrap_or(value);
                            hits.push(Hit { address: pos + i as u64, value_type: *ty, scale: *scale, previous });
                        }
                        i += align;
                    }
                }
            }
            pos += stride as u64;
        }
    }
    // next_scan agrupa per adreça, també quan els candidats tenen tipus diferents.
    // A la mateixa adreça, primer el tipus més petit: un i32 en cèntims també es llegeix com a
    // i64 si els bytes següents són zero, però el més habitual és l'i32
    hits.sort_by_key(|h| (h.address, h.value_type.size()));
    ScanState { hits, truncated, scans: 1, read_failures: 0 }
}

impl ScanState {
    pub fn count(&self) -> usize {
        self.hits.len()
    }

    /// Rellegeix els resultats actuals i es queda amb els que compleixen `filter`.
    /// Els resultats propers es llegeixen junts en blocs per fer menys crides al sistema.
    pub fn next_scan(&mut self, mem: &dyn Memory, filter: ScanFilter) -> AppResult<()> {
        let mut kept = Vec::with_capacity(self.hits.len() / 4);
        self.read_failures = 0;
        let mut i = 0;
        while i < self.hits.len() {
            let start = self.hits[i].address;
            let mut j = i;
            let mut block_end = start;
            while j < self.hits.len() && self.hits[j].address + self.hits[j].value_type.size() as u64 <= start + BLOCK {
                block_end = block_end.max(self.hits[j].address + self.hits[j].value_type.size() as u64);
                j += 1;
            }
            let mut buf = vec![0u8; (block_end - start) as usize];
            let block_ok = mem.read_bytes(start, &mut buf).is_ok();
            for h in &self.hits[i..j] {
                let (ty, size) = (h.value_type, h.value_type.size());
                let now = if block_ok {
                    let off = (h.address - start) as usize;
                    ty.decode(&buf[off..off + size]).ok()
                } else {
                    // El bloc creuava una zona no llegible: un per un
                    let mut b = vec![0u8; size];
                    mem.read_bytes(h.address, &mut b).ok().and_then(|_| ty.decode(&b).ok())
                }
                .map(|raw| raw / h.scale);
                match now {
                    Some(now) if keep(ty, h.scale, filter, now, h.previous) => {
                        kept.push(Hit { previous: now, ..*h });
                    }
                    None => self.read_failures += 1,
                    _ => {}
                }
            }
            i = j;
        }
        if self.read_failures > 0 {
            tracing::warn!(failed = self.read_failures, total = self.hits.len(), "candidats no llegibles en refinar");
        }
        if !self.hits.is_empty() && self.read_failures == self.hits.len() {
            return Err(AppError::InvalidValue("No s'ha pogut rellegir cap candidat. Comprova que el joc continua obert i torna-ho a provar; hem conservat la cerca anterior.".into()));
        }
        self.hits = kept;
        self.scans += 1;
        Ok(())
    }

    /// Una pàgina de resultats amb el valor actual.
    pub fn page(&self, mem: &dyn Memory, modules: &[ModuleInfo], offset: usize, limit: usize) -> Vec<ScanHit> {
        self.hits
            .iter()
            .skip(offset)
            .take(limit)
            .map(|h| {
                let size = h.value_type.size();
                let mut b = vec![0u8; size];
                let value = mem.read_bytes(h.address, &mut b).ok().and_then(|_| h.value_type.decode(&b).ok()).map(|raw| raw / h.scale);
                ScanHit {
                    address: format!("{:#x}", h.address),
                    value_type: h.value_type,
                    scale: h.scale,
                    module_offset: module_offset(modules, h.address).map(|(m, o)| format!("{}+{o:#x}", m.name)),
                    value,
                    previous: h.previous,
                }
            })
            .collect()
    }
}

/// Mòdul que conté `address` i l'offset dins seu.
pub fn module_offset(modules: &[ModuleInfo], address: u64) -> Option<(&ModuleInfo, u64)> {
    modules.iter().find(|m| address >= m.base && address < m.base + m.size).map(|m| (m, address - m.base))
}

/// Possible camí estàtic cap a una adreça dinàmica: `[mòdul+moduleOffset] + pointerOffset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PointerCandidate {
    pub module: String,
    /// On és el punter dins del mòdul (hex)
    pub module_offset: String,
    /// Què s'ha de sumar al punter per arribar al valor (hex)
    pub pointer_offset: String,
}

/// Cerca de punters d'**un nivell**: busca, dins les dades dels mòduls (variables globals),
/// punters que apuntin a l'adreça o una mica abans (`target - max_offset ..= target`).
/// És el cas típic «variable global → objecte → camp». Les cadenes més llargues s'han
/// de trobar amb eines externes i escriure a mà.
pub fn find_static_pointers(
    mem: &dyn Memory,
    modules: &[ModuleInfo],
    target: u64,
    max_offset: u64,
    max_results: usize,
) -> Vec<PointerCandidate> {
    let ps = mem.pointer_size();
    let mut found: Vec<(u64, PointerCandidate)> = Vec::new();
    for m in modules {
        for r in mem.regions(m.base, m.base + m.size).iter().filter(|r| scannable(r)) {
            let (from, to) = (r.base.max(m.base), r.end().min(m.base + m.size));
            let mut buf = vec![0u8; (to - from) as usize];
            if mem.read_bytes(from, &mut buf).is_err() {
                continue;
            }
            for (k, chunk) in buf.chunks_exact(ps).enumerate() {
                let mut v = [0u8; 8];
                v[..ps].copy_from_slice(chunk);
                let ptr = u64::from_le_bytes(v);
                if ptr != 0 && ptr <= target && target - ptr <= max_offset {
                    let slot = from + (k * ps) as u64;
                    found.push((
                        target - ptr,
                        PointerCandidate {
                            module: m.name.clone(),
                            module_offset: format!("{:#x}", slot - m.base),
                            pointer_offset: format!("{:#x}", target - ptr),
                        },
                    ));
                }
            }
        }
    }
    // Primer els offsets petits: és més probable que siguin el camp d'un objecte
    found.sort_by_key(|(off, c)| (*off, c.module.clone()));
    found.into_iter().take(max_results).map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::fake::FakeMemory;

    fn put_i32(m: &FakeMemory, a: u64, v: i32) {
        m.put(a, &v.to_le_bytes());
    }

    #[test]
    fn automatic_scan_keeps_double_money_after_float_coincidences_disappear() {
        let m = FakeMemory::new(0x10000, 0x100, 8);
        m.put(0x10010, &6.71f64.to_le_bytes());
        m.put(0x10040, &6.71f32.to_le_bytes()); // coincidència que no són els diners
        let mut old = first_scan_in(&m, ValueType::F32, 6.71, 0x10000, u64::MAX, 100);
        let mut auto = first_scan_types_in(&m, &AUTO_TYPES, 6.71, 0x10000, u64::MAX, 100);
        assert!(old.count() > 0);
        assert!(auto.count() > old.count());

        m.put(0x10010, &6.86f64.to_le_bytes());
        old.next_scan(&m, ScanFilter::Exact { value: 6.86 }).unwrap();
        assert_eq!(old.count(), 0, "reprodueix el problema de cercar només f32");
        auto.next_scan(&m, ScanFilter::Exact { value: 6.86 }).unwrap();
        let hits = auto.page(&m, &[], 0, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].address, "0x10010");
        assert_eq!(hits[0].value_type, ValueType::F64);
        assert_eq!(hits[0].value, Some(6.86));
        // El tipus retornat a la UI permet provar el candidat sense escriure només mitja dada.
        m.put(0x10010, &hits[0].value_type.encode(999.0).unwrap());
        assert_eq!(auto.page(&m, &[], 0, 1)[0].value, Some(999.0));
    }

    #[test]
    fn automatic_scan_finds_money_stored_as_cents() {
        let m = FakeMemory::new(0x10000, 0x100, 8);
        m.put(0x10020, &4770i32.to_le_bytes()); // 47,70 a la pantalla
        let mut s = first_scan_auto_in(&m, 47.70);
        m.put(0x10020, &4986i32.to_le_bytes()); // guanya 2,16
        s.next_scan(&m, ScanFilter::Exact { value: 49.86 }).unwrap();
        let hits = s.page(&m, &[], 0, 10);
        // L'i32 en cèntims, i el mateix lloc llegit com a i64 (els bytes de després són zero)
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert_eq!((hits[0].value_type, hits[0].scale, hits[0].value), (ValueType::I32, 100.0, Some(49.86)), "primer el més petit");
        assert_eq!((hits[1].value_type, hits[1].address.as_str()), (ValueType::I64, "0x10020"));
        s.next_scan(&m, ScanFilter::Unchanged).unwrap();
        assert_eq!(s.count(), 2);
        m.put(0x10020, &4985i32.to_le_bytes());
        s.next_scan(&m, ScanFilter::Decreased).unwrap();
        assert_eq!(s.count(), 2, "un cèntim menys també compta");
        assert!(auto_targets(100.0).iter().all(|(_, sc)| *sc == 1.0), "sense decimals no busquem cèntims");
    }

    fn first_scan_auto_in(m: &FakeMemory, v: f64) -> ScanState {
        first_scan_targets_in(m, &auto_targets(v), v, 0x10000, u64::MAX, 100)
    }

    #[test]
    fn automatic_scan_tracks_mixed_types_and_sorts_hits_for_block_reads() {
        let m = FakeMemory::new(0x10000, 0x200, 8);
        let fields = [(0x10010, ValueType::I32), (0x10040, ValueType::F32),
            (0x10080, ValueType::I64), (0x10100, ValueType::F64)];
        for (addr, ty) in fields { m.put(addr, &ty.encode(100.0).unwrap()); }
        let mut s = first_scan_types_in(&m, &AUTO_TYPES, 100.0, 0x10000, u64::MAX, 100);
        for (addr, ty) in fields { m.put(addr, &ty.encode(85.0).unwrap()); }
        s.next_scan(&m, ScanFilter::Decreased).unwrap();
        s.next_scan(&m, ScanFilter::Exact { value: 85.0 }).unwrap();
        s.next_scan(&m, ScanFilter::Unchanged).unwrap();
        let hits = s.page(&m, &[], 0, 100);
        for (addr, ty) in fields {
            assert!(hits.iter().any(|h| h.address == format!("{addr:#x}") && h.value_type == ty && h.value == Some(85.0)));
        }
    }

    #[test]
    fn double_crossing_chunk_boundary_is_not_lost_or_duplicated() {
        let m = FakeMemory::new(0x10000, CHUNK + 16, 8);
        let addr = 0x10000 + CHUNK as u64 - 4;
        m.put(addr, &6.71f64.to_le_bytes());
        let s = first_scan_in(&m, ValueType::F64, 6.71, 0x10000, u64::MAX, 100);
        assert_eq!(s.count(), 1);
        assert_eq!(s.page(&m, &[], 0, 1)[0].address, format!("{addr:#x}"));
    }

    #[test]
    fn failed_reads_preserve_scan_for_retry_and_partial_failures_are_reported() {
        let m = FakeMemory::new(0x10000, 0x100, 8);
        put_i32(&m, 0x10010, 100);
        put_i32(&m, 0x10080, 100);
        let mut s = first_scan_in(&m, ValueType::I32, 100.0, 0x10000, u64::MAX, 100);
        let unavailable = FakeMemory::new(0x20000, 0x100, 8);
        assert!(s.next_scan(&unavailable, ScanFilter::Unchanged).is_err());
        assert_eq!(s.count(), 2);
        assert_eq!(s.scans, 1);
        assert_eq!(s.read_failures, 2);
        s.next_scan(&m, ScanFilter::Unchanged).unwrap();
        assert_eq!(s.count(), 2);
        assert_eq!(s.read_failures, 0);
        m.data.lock().unwrap().truncate(0x40);
        s.next_scan(&m, ScanFilter::Unchanged).unwrap();
        assert_eq!(s.count(), 1, "el fallback individual conserva el candidat llegible");
        assert_eq!(s.read_failures, 1);
    }

    #[test]
    fn exact_then_decreased_narrows_to_one() {
        let m = FakeMemory::new(0x10000, 0x1000, 8);
        for a in [0x10010, 0x10100, 0x10200, 0x10FFC] {
            put_i32(&m, a, 100);
        }
        let mut s = first_scan_in(&m, ValueType::I32, 100.0, 0x10000, u64::MAX, 100);
        assert_eq!(s.count(), 4, "inclou l'últim valor de la regió");

        put_i32(&m, 0x10100, 85); // "rep mal"
        put_i32(&m, 0x10200, 130); // un altre valor que puja
        s.next_scan(&m, ScanFilter::Decreased).unwrap();
        assert_eq!(s.page(&m, &[], 0, 10).iter().map(|h| h.address.as_str()).collect::<Vec<_>>(), ["0x10100"]);
        assert_eq!(s.page(&m, &[], 0, 10)[0].previous, 85.0);

        s.next_scan(&m, ScanFilter::Unchanged).unwrap();
        assert_eq!(s.count(), 1);
        put_i32(&m, 0x10100, 70);
        s.next_scan(&m, ScanFilter::Exact { value: 60.0 }).unwrap();
        assert_eq!(s.count(), 0);
        assert_eq!(s.scans, 4);
    }

    #[test]
    fn increased_changed_and_float_tolerance() {
        let m = FakeMemory::new(0x10000, 0x100, 8);
        m.put(0x10000, &1.5f32.to_le_bytes());
        m.put(0x10004, &1.5f32.to_le_bytes());
        let mut s = first_scan_in(&m, ValueType::F32, 1.5, 0x10000, u64::MAX, 100);
        assert_eq!(s.count(), 2);
        m.put(0x10004, &3.0f32.to_le_bytes());
        s.next_scan(&m, ScanFilter::Changed).unwrap();
        assert_eq!(s.count(), 1);
        s.next_scan(&m, ScanFilter::Increased).unwrap();
        assert_eq!(s.count(), 0, "ja no ha pujat més des de l'última cerca");
        assert!(equals_scaled(ValueType::F32, 1.0, 100.004, 100.0));
        assert!(!equals_scaled(ValueType::I32, 1.0, 101.0, 100.0));
    }

    #[test]
    fn respects_hit_limit_and_skips_read_only_memory() {
        let m = FakeMemory::new(0x10000, 0x100, 8);
        let s = first_scan_in(&m, ValueType::I32, 0.0, 0x10000, u64::MAX, 5);
        assert_eq!(s.count(), 5);
        assert!(s.truncated);
        let ro = FakeMemory { writable: false, ..FakeMemory::new(0x10000, 0x100, 8) };
        assert_eq!(first_scan_in(&ro, ValueType::I32, 0.0, 0x10000, u64::MAX, 5).count(), 0);
    }

    #[test]
    fn finds_static_pointer_to_heap_object() {
        // "Mòdul" a 0x40000; una global a +0x100 apunta a l'objecte (0x40180); el valor és a +0x8
        let m = FakeMemory::new(0x40000, 0x200, 8);
        m.put_ptr(0x40100, 0x40180);
        let mods = [ModuleInfo { name: "joc.exe".into(), base: 0x40000, size: 0x200 }];
        let c = find_static_pointers(&m, &mods, 0x40188, 0x100, 5);
        assert_eq!(
            c.first(),
            Some(&PointerCandidate { module: "joc.exe".into(), module_offset: "0x100".into(), pointer_offset: "0x8".into() })
        );
        assert!(find_static_pointers(&m, &mods, 0x40188, 0x4, 5).is_empty(), "massa lluny");
        assert_eq!(module_offset(&mods, 0x40188).map(|(_, o)| o), Some(0x188));
        assert!(module_offset(&mods, 0x50000).is_none());
    }
}
