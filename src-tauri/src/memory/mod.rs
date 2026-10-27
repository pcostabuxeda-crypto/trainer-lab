//! Accés a la memòria d'altres processos.
//!
//! Tot passa per la trait [`Memory`]. Hi ha dues implementacions:
//! - [`process::Process`]: un procés real (ReadProcessMemory / WriteProcessMemory).
//! - `fake::FakeMemory`: un buffer simulat per als tests.
//!
//! Així els pointer chains, l'AOB i la congelació es poden provar sense cap joc obert.

pub mod aob;
#[cfg(test)]
pub mod fake;
pub mod freeze;
pub mod locator;
pub mod pointer;
pub mod process;
pub mod scanner;
pub mod value;

use crate::error::{AppError, AppResult};

/// Una regió de memòria virtual del procés (el que retorna `VirtualQueryEx`).
/// Windows reparteix la memòria en regions contigües amb el mateix estat i protecció.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub base: u64,
    pub size: u64,
    /// `MEM_COMMIT`: la regió té memòria física/arxiu de paginació assignada
    pub committed: bool,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    /// `PAGE_GUARD`: tocar-la llança una excepció dins del joc. Mai hi escrivim.
    pub guard: bool,
}

impl Region {
    pub fn end(&self) -> u64 {
        self.base.saturating_add(self.size)
    }
}

/// Com s'escriu: dades normals o codi (que cal desprotegir temporalment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Data,
    Code,
}

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub base: u64,
    pub size: u64,
}

pub trait Memory: Send + Sync {
    /// Llegeix exactament `buf.len()` bytes o falla.
    fn read_bytes(&self, address: u64, buf: &mut [u8]) -> AppResult<()>;
    /// Escriu després de validar la regió amb [`check_write`].
    fn write_bytes(&self, address: u64, data: &[u8], mode: WriteMode) -> AppResult<()>;
    /// 4 en processos x86 (WoW64), 8 en x64.
    fn pointer_size(&self) -> usize;
    /// Regió que conté `address`, o `None` si no és vàlida.
    fn region(&self, address: u64) -> Option<Region>;

    /// Regions entre `start` i `end` (per escanejar un mòdul).
    fn regions(&self, start: u64, end: u64) -> Vec<Region> {
        let mut out = Vec::new();
        let mut addr = start;
        while addr < end {
            let Some(r) = self.region(addr) else { break };
            if r.size == 0 {
                break;
            }
            out.push(r);
            addr = r.end();
        }
        out
    }

    fn read_pointer(&self, address: u64) -> AppResult<u64> {
        let mut b = [0u8; 8];
        let n = self.pointer_size();
        self.read_bytes(address, &mut b[..n])?;
        Ok(u64::from_le_bytes(b))
    }
}

/// Validació abans de qualsevol escriptura. És pura (no toca cap procés) i es testeja sola.
pub fn check_write(region: Option<Region>, address: u64, len: usize, mode: WriteMode) -> AppResult<()> {
    let reject = |reason: &str| Err(AppError::WriteRejected { address, reason: reason.into() });
    if len == 0 {
        return reject("no hi ha bytes per escriure");
    }
    if address < 0x10000 {
        return reject("adreça massa baixa (probablement un punter nul)");
    }
    let Some(r) = region else { return reject("l'adreça no pertany a cap regió vàlida") };
    if !r.committed {
        return reject("la regió no té memòria assignada");
    }
    if r.guard {
        return reject("la regió és una pàgina de guarda");
    }
    let end = address.checked_add(len as u64);
    if address < r.base || end.is_none_or(|e| e > r.end()) {
        return reject("l'escriptura sortiria de la regió");
    }
    match mode {
        WriteMode::Data if !r.writable => reject("la regió no és escrivible (fes servir un patch de codi)"),
        WriteMode::Code if !r.executable && !r.writable => reject("la regió no és ni codi ni dades escrivibles"),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rw() -> Region {
        Region { base: 0x10000, size: 0x1000, committed: true, readable: true, writable: true, executable: false, guard: false }
    }

    #[test]
    fn accepts_write_inside_writable_region() {
        assert!(check_write(Some(rw()), 0x10010, 4, WriteMode::Data).is_ok());
        assert!(check_write(Some(rw()), 0x10FFC, 4, WriteMode::Data).is_ok());
    }

    #[test]
    fn rejects_bad_writes() {
        let bad = |r: Option<Region>, a: u64, n: usize, m: WriteMode| check_write(r, a, n, m).is_err();
        assert!(bad(None, 0x10010, 4, WriteMode::Data), "sense regió");
        assert!(bad(Some(rw()), 0x10FFE, 4, WriteMode::Data), "desborda la regió");
        assert!(bad(Some(rw()), 0x10, 4, WriteMode::Data), "adreça nul·la");
        assert!(bad(Some(rw()), 0x10010, 0, WriteMode::Data), "zero bytes");
        assert!(bad(Some(Region { committed: false, ..rw() }), 0x10010, 4, WriteMode::Data), "sense commit");
        assert!(bad(Some(Region { guard: true, ..rw() }), 0x10010, 4, WriteMode::Data), "pàgina de guarda");
        let code = Region { writable: false, executable: true, ..rw() };
        assert!(bad(Some(code), 0x10010, 4, WriteMode::Data), "dades sobre codi");
        assert!(check_write(Some(code), 0x10010, 4, WriteMode::Code).is_ok(), "patch sobre codi");
    }
}
