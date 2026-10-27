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

use super::value::ValueType;
use super::{Memory, ModuleInfo, Region};

/// Bytes que es llegeixen de cop a la primera cerca.
const CHUNK: usize = 4 << 20;
/// Bloc màxim per rellegir resultats propers a les cerques següents.
const BLOCK: u64 = 64 << 10;
/// Límit de resultats guardats (16 bytes cadascun: ~32 MB).
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
    /// Valor a la cerca anterior
    previous: f64,
}

pub struct ScanState {
    pub value_type: ValueType,
    hits: Vec<Hit>,
    pub truncated: bool,
    pub scans: u32,
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
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanHit {
    pub address: String,
    /// `mòdul+offset` si l'adreça és dins d'un mòdul (estàtica); `None` si és dinàmica (heap)
    pub module_offset: Option<String>,
    pub value: Option<f64>,
    pub previous: f64,
}

fn alignment(ty: ValueType) -> usize {
    ty.size().clamp(1, 4)
}

/// Igualtat "humana": els decimals amb una petita tolerància (100.0001 és 100).
fn equals(ty: ValueType, a: f64, b: f64) -> bool {
    match ty {
        ValueType::F32 | ValueType::F64 => (a - b).abs() < 0.01,
        _ => a == b,
    }
}

fn keep(ty: ValueType, filter: ScanFilter, now: f64, before: f64) -> bool {
    match filter {
        ScanFilter::Exact { value } => equals(ty, now, value),
        ScanFilter::Changed => !equals(ty, now, before),
        ScanFilter::Unchanged => equals(ty, now, before),
        ScanFilter::Increased => now > before && !equals(ty, now, before),
        ScanFilter::Decreased => now < before && !equals(ty, now, before),
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
    let (size, align) = (ty.size(), alignment(ty));
    let is_match = matcher(ty, value);
    let mut hits = Vec::new();
    let mut truncated = false;
    'regions: for r in mem.regions(start, end).iter().filter(|r| scannable(r)) {
        let mut pos = r.base.max(start);
        let to = r.end().min(end);
        while pos < to {
            let len = (CHUNK as u64).min(to - pos) as usize;
            let mut buf = vec![0u8; len];
            if mem.read_bytes(pos, &mut buf).is_ok() {
                // `pos` sempre és múltiple de 4 KB (inici de regió + blocs de 4 MB): les
                // posicions alineades dins del buffer també ho són a la memòria
                let mut i = 0;
                while i + size <= len {
                    let bytes = &buf[i..i + size];
                    if is_match(bytes) {
                        if hits.len() >= max {
                            truncated = true;
                            break 'regions;
                        }
                        let previous = ty.decode(bytes).unwrap_or(value);
                        hits.push(Hit { address: pos + i as u64, previous });
                    }
                    i += align;
                }
            }
            pos += len as u64;
        }
    }
    ScanState { value_type: ty, hits, truncated, scans: 1 }
}

impl ScanState {
    pub fn count(&self) -> usize {
        self.hits.len()
    }

    /// Rellegeix els resultats actuals i es queda amb els que compleixen `filter`.
    /// Els resultats propers es llegeixen junts en blocs per fer menys crides al sistema.
    pub fn next_scan(&mut self, mem: &dyn Memory, filter: ScanFilter) {
        let (ty, size) = (self.value_type, self.value_type.size());
        let mut kept = Vec::with_capacity(self.hits.len() / 4);
        let mut i = 0;
        while i < self.hits.len() {
            let start = self.hits[i].address;
            let mut j = i;
            while j < self.hits.len() && self.hits[j].address + size as u64 <= start + BLOCK {
                j += 1;
            }
            let block_end = self.hits[j - 1].address + size as u64;
            let mut buf = vec![0u8; (block_end - start) as usize];
            let block_ok = mem.read_bytes(start, &mut buf).is_ok();
            for h in &self.hits[i..j] {
                let now = if block_ok {
                    let off = (h.address - start) as usize;
                    ty.decode(&buf[off..off + size]).ok()
                } else {
                    // El bloc creuava una zona no llegible: un per un
                    let mut b = vec![0u8; size];
                    mem.read_bytes(h.address, &mut b).ok().and_then(|_| ty.decode(&b).ok())
                };
                if let Some(now) = now
                    && keep(ty, filter, now, h.previous)
                {
                    kept.push(Hit { address: h.address, previous: now });
                }
            }
            i = j;
        }
        self.hits = kept;
        self.scans += 1;
    }

    /// Una pàgina de resultats amb el valor actual.
    pub fn page(&self, mem: &dyn Memory, modules: &[ModuleInfo], offset: usize, limit: usize) -> Vec<ScanHit> {
        let size = self.value_type.size();
        self.hits
            .iter()
            .skip(offset)
            .take(limit)
            .map(|h| {
                let mut b = vec![0u8; size];
                let value = mem.read_bytes(h.address, &mut b).ok().and_then(|_| self.value_type.decode(&b).ok());
                ScanHit {
                    address: format!("{:#x}", h.address),
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
    fn exact_then_decreased_narrows_to_one() {
        let m = FakeMemory::new(0x10000, 0x1000, 8);
        for a in [0x10010, 0x10100, 0x10200, 0x10FFC] {
            put_i32(&m, a, 100);
        }
        let mut s = first_scan_in(&m, ValueType::I32, 100.0, 0x10000, u64::MAX, 100);
        assert_eq!(s.count(), 4, "inclou l'últim valor de la regió");

        put_i32(&m, 0x10100, 85); // "rep mal"
        put_i32(&m, 0x10200, 130); // un altre valor que puja
        s.next_scan(&m, ScanFilter::Decreased);
        assert_eq!(s.page(&m, &[], 0, 10).iter().map(|h| h.address.as_str()).collect::<Vec<_>>(), ["0x10100"]);
        assert_eq!(s.page(&m, &[], 0, 10)[0].previous, 85.0);

        s.next_scan(&m, ScanFilter::Unchanged);
        assert_eq!(s.count(), 1);
        put_i32(&m, 0x10100, 70);
        s.next_scan(&m, ScanFilter::Exact { value: 60.0 });
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
        s.next_scan(&m, ScanFilter::Changed);
        assert_eq!(s.count(), 1);
        s.next_scan(&m, ScanFilter::Increased);
        assert_eq!(s.count(), 0, "ja no ha pujat més des de l'última cerca");
        assert!(equals(ValueType::F32, 100.004, 100.0));
        assert!(!equals(ValueType::I32, 101.0, 100.0));
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
