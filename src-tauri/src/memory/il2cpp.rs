//! Localitza camps de jocs Unity IL2CPP pel nom, llegint des de fora les estructures de
//! l'IL2CPP. Els noms sobreviuen a les actualitzacions del joc; el codi i els offsets, no.
//!
//! Cada camp d'una classe carregada té un `FieldInfo` al heap: punter al nom (dins de
//! `global-metadata.dat`), tipus, classe i offset. Es busca el nom i després qui hi apunta.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Memory;
use super::scanner::{CHUNK, USER_END, scannable};
use crate::error::{AppError, AppResult};

const FIELD_PARENT: u64 = 0x10;
const FIELD_OFFSET: u64 = 0x18;
const CLASS_NAME: u64 = 0x10;
/// `static_fields` canvia de lloc segons la versió d'IL2CPP: es busca dins d'aquest rang.
const STATIC_FIELDS: std::ops::Range<u64> = 0x80..0x100;

/// Camí per noms, tots en format `Classe.camp` (sense espai de noms).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct Il2cppPath {
    /// Camp estàtic on comença el camí: el singleton del joc
    pub root: String,
    /// Camps a seguir a partir de l'objecte, l'últim és el valor
    pub fields: Vec<String>,
}

impl Il2cppPath {
    /// `(classe, camp)` de cada pas, l'arrel primer.
    pub fn parts(&self) -> AppResult<Vec<(&str, &str)>> {
        std::iter::once(&self.root)
            .chain(&self.fields)
            .map(|s| s.split_once('.').ok_or_else(|| AppError::InvalidValue(format!("«{s}» no és `Classe.camp`"))))
            .collect()
    }
}

fn ptr(mem: &dyn Memory, address: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    mem.read_bytes(address, &mut b).ok()?;
    Some(u64::from_le_bytes(b))
}

/// Adreces dels noms a la metadata (fitxer mapejat), ordenades, amb el nom que hi ha.
fn find_names<'a>(mem: &dyn Memory, names: &[&'a str]) -> Vec<(u64, &'a str)> {
    let needles: Vec<(Vec<u8>, &str)> = names.iter().map(|n| ([b"\0", n.as_bytes(), b"\0"].concat(), *n)).collect();
    let overlap = needles.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    let mut found = Vec::new();
    for r in mem.regions(0x10000, USER_END).iter().filter(|r| r.mapped && r.committed && r.readable && !r.guard) {
        let mut pos = r.base;
        while pos < r.end() {
            let stride = (CHUNK as u64).min(r.end() - pos) as usize;
            let mut buf = vec![0u8; (stride + overlap).min((r.end() - pos) as usize)];
            if mem.read_bytes(pos, &mut buf).is_ok() {
                for (needle, name) in &needles {
                    let hits = buf.windows(needle.len()).enumerate().filter(|(i, w)| *i < stride && w == needle);
                    found.extend(hits.map(|(i, _)| (pos + i as u64 + 1, *name)));
                }
            }
            pos += stride as u64;
        }
    }
    found.sort_unstable();
    found
}

/// Classe i offset de cada pas, recorrent el heap a la recerca de `FieldInfo`.
fn find_fields(mem: &dyn Memory, wanted: &[(&str, &str)], names: &[(u64, &str)]) -> Vec<Option<(u64, i32)>> {
    let mut out = vec![None; wanted.len()];
    let (Some(first), Some(last)) = (names.first(), names.last()) else { return out };
    let mut check = |field_info: u64, name: &str| {
        for (i, (class, field)) in wanted.iter().enumerate() {
            if *field != name || out[i].is_some() {
                continue;
            }
            let Some(klass) = ptr(mem, field_info + FIELD_PARENT) else { continue };
            let Some(class_name) = ptr(mem, klass + CLASS_NAME) else { continue };
            let mut b = vec![0u8; class.len() + 1];
            let mut offset = [0u8; 4];
            if mem.read_bytes(class_name, &mut b).is_ok()
                && b[..class.len()] == *class.as_bytes()
                && b[class.len()] == 0
                && mem.read_bytes(field_info + FIELD_OFFSET, &mut offset).is_ok()
            {
                out[i] = Some((klass, i32::from_le_bytes(offset)));
            }
        }
    };
    for r in mem.regions(0x10000, USER_END).iter().filter(|r| scannable(r)) {
        let mut pos = r.base;
        while pos < r.end() {
            let len = (CHUNK as u64).min(r.end() - pos) as usize;
            let mut buf = vec![0u8; len];
            if mem.read_bytes(pos, &mut buf).is_ok() {
                for (i, q) in buf.chunks_exact(8).enumerate() {
                    let v = u64::from_le_bytes(q.try_into().unwrap());
                    // El filtre de rang estalvia la cerca binària en gairebé totes les paraules
                    if v >= first.0 && v <= last.0
                        && let Ok(k) = names.binary_search_by_key(&v, |n| n.0)
                    {
                        check(pos + (i * 8) as u64, names[k].1);
                    }
                }
            }
            pos += len as u64;
        }
    }
    out
}

/// On és el punter del singleton: `static_fields + offset`.
// shortcut: només arrels singleton (l'objecte és de la mateixa classe); per a altres estàtics caldria conèixer `static_fields` per versió d'IL2CPP.
fn static_slot(mem: &dyn Memory, klass: u64, offset: i32) -> Option<u64> {
    let offset = u64::try_from(offset).ok()?;
    STATIC_FIELDS.step_by(8).find_map(|o| {
        let slot = ptr(mem, klass + o)?.checked_add(offset)?;
        let object = ptr(mem, slot)?;
        (object != 0 && ptr(mem, object) == Some(klass)).then_some(slot)
    })
}

/// Punt de partida i offsets de la cadena. Car: uns segons sobre tota la memòria del joc.
pub fn resolve(mem: &dyn Memory, path: &Il2cppPath) -> AppResult<(u64, Vec<i64>)> {
    if mem.pointer_size() != 8 {
        return Err(AppError::InvalidValue("els camins IL2CPP només funcionen en jocs de 64 bits".into()));
    }
    let wanted = path.parts()?;
    let mut names: Vec<&str> = wanted.iter().map(|(_, f)| *f).collect();
    names.sort_unstable();
    names.dedup();
    let found = find_fields(mem, &wanted, &find_names(mem, &names));
    let not_loaded = |name: &str| AppError::InvalidValue(format!("Encara no s'ha carregat {name}: entra a la partida i torna-ho a provar."));
    let mut steps = Vec::with_capacity(found.len());
    for (f, (class, field)) in found.iter().zip(&wanted) {
        steps.push(f.ok_or_else(|| not_loaded(&format!("{class}.{field}")))?);
    }
    let (klass, offset) = steps[0];
    let slot = static_slot(mem, klass, offset).ok_or_else(|| not_loaded(&path.root))?;
    Ok((slot, steps[1..].iter().map(|(_, o)| *o as i64).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::fake::FakeMemory;
    use crate::memory::{Region, WriteMode, pointer};

    /// Metadata mapejada + heap, com en un joc de debò.
    struct Game {
        metadata: FakeMemory,
        heap: FakeMemory,
    }

    impl Game {
        fn at(&self, address: u64) -> &FakeMemory {
            if address >= self.heap.base { &self.heap } else { &self.metadata }
        }
    }

    impl Memory for Game {
        fn read_bytes(&self, address: u64, buf: &mut [u8]) -> AppResult<()> {
            self.at(address).read_bytes(address, buf)
        }
        fn write_bytes(&self, address: u64, data: &[u8], mode: WriteMode) -> AppResult<()> {
            self.at(address).write_bytes(address, data, mode)
        }
        fn pointer_size(&self) -> usize {
            8
        }
        fn region(&self, address: u64) -> Option<Region> {
            self.at(address).region(address).or_else(|| (address < self.heap.base).then(|| self.heap.region(self.heap.base)).flatten())
        }
    }

    const META: u64 = 0x10000;
    const HEAP: u64 = 0x20000;

    fn field_info(g: &Game, at: u64, name: u64, klass: u64, offset: i32) {
        g.heap.put_ptr(at, name);
        g.heap.put_ptr(at + FIELD_PARENT, klass);
        g.heap.put(at + FIELD_OFFSET, &offset.to_le_bytes());
    }

    fn class(g: &Game, at: u64, name: u64) {
        g.heap.put_ptr(at + CLASS_NAME, name);
    }

    /// Sessió (singleton, +0x30 = món) → Món (+0x48 = moneder) → Moneder (+0x10 = saldo).
    fn game() -> Game {
        let mut metadata = FakeMemory::new(META, 0x100, 8);
        metadata.mapped = true;
        metadata.writable = false;
        let g = Game { metadata, heap: FakeMemory::new(HEAP, 0x1000, 8) };
        for (at, s) in [(0x10u64, "Session"), (0x20, "Instance"), (0x30, "world"), (0x40, "World"), (0x50, "wallets"), (0x60, "Wallet"), (0x70, "Balance"), (0x80, "Other")] {
            g.metadata.put(META + at, s.as_bytes());
        }
        let (session, world, wallet, other) = (HEAP + 0x100, HEAP + 0x200, HEAP + 0x300, HEAP + 0x400);
        class(&g, session, META + 0x10);
        class(&g, world, META + 0x40);
        class(&g, wallet, META + 0x60);
        class(&g, other, META + 0x80);
        field_info(&g, HEAP + 0x500, META + 0x20, session, 8);
        field_info(&g, HEAP + 0x520, META + 0x30, session, 0x30);
        field_info(&g, HEAP + 0x540, META + 0x50, other, 0x99); // mateix nom, una altra classe
        field_info(&g, HEAP + 0x560, META + 0x50, world, 0x48);
        field_info(&g, HEAP + 0x580, META + 0x70, wallet, 0x10);
        // static_fields de Session a +0x98; el singleton, 8 bytes més enllà
        g.heap.put_ptr(session + 0x98, HEAP + 0x600);
        let (obj, world_obj, wallet_obj) = (HEAP + 0x700, HEAP + 0x800, HEAP + 0x900);
        g.heap.put_ptr(HEAP + 0x608, obj);
        g.heap.put_ptr(obj, session);
        g.heap.put_ptr(obj + 0x30, world_obj);
        g.heap.put_ptr(world_obj + 0x48, wallet_obj);
        g.heap.put(wallet_obj + 0x10, &[7; 4]);
        g
    }

    fn path() -> Il2cppPath {
        Il2cppPath { root: "Session.Instance".into(), fields: vec!["Session.world".into(), "World.wallets".into(), "Wallet.Balance".into()] }
    }

    #[test]
    fn follows_names_to_the_value() {
        let g = game();
        let (start, offsets) = resolve(&g, &path()).unwrap();
        assert_eq!((start, offsets.as_slice()), (HEAP + 0x608, [0x30, 0x48, 0x10].as_slice()));
        assert_eq!(pointer::resolve(&g, start, &offsets).unwrap(), HEAP + 0x910);
    }

    #[test]
    fn explains_what_is_not_loaded_yet() {
        let g = game();
        g.heap.put_ptr(HEAP + 0x608, 0); // el singleton encara no existeix
        assert!(resolve(&g, &path()).unwrap_err().to_string().contains("Session.Instance"));
        let missing = Il2cppPath { root: "Session.Instance".into(), fields: vec!["Session.nope".into()] };
        assert!(resolve(&game(), &missing).unwrap_err().to_string().contains("Session.nope"));
        assert!(Il2cppPath { root: "sensepunt".into(), fields: vec![] }.parts().is_err());
    }
}
