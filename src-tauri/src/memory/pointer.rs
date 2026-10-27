//! Pointer chains (cadenes de punters).
//!
//! Els jocs guarden la majoria de dades en memòria dinàmica (heap), que canvia d'adreça a
//! cada partida. Però sempre hi ha algun lloc fix (una variable estàtica de l'executable)
//! que apunta a una estructura, que apunta a una altra... fins al valor.
//!
//! Notació (la mateixa que Cheat Engine): `[[base] + 0x10] + 0x48`
//! ```text
//! addr = base
//! per a cada offset:  addr = llegeix_punter(addr) + offset
//! ```
//! El resultat és l'adreça del valor (no el valor). A x86 els punters fan 4 bytes; a x64, 8.

use super::Memory;
use crate::error::{AppError, AppResult};

pub fn resolve(mem: &dyn Memory, base: u64, offsets: &[i64]) -> AppResult<u64> {
    let mut addr = base;
    for (step, &off) in offsets.iter().enumerate() {
        let ptr = mem.read_pointer(addr)?;
        if ptr == 0 {
            return Err(AppError::NullPointer { step, address: addr });
        }
        addr = ptr.wrapping_add_signed(off);
    }
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::fake::FakeMemory;

    // Memòria simulada: [0x1000] -> 0x1100, [0x1100 + 0x18] -> 0x1200, valor a 0x1200 + 0x40
    fn chain(ptr_size: usize) -> FakeMemory {
        let m = FakeMemory::new(0x1000, 0x400, ptr_size);
        m.put_ptr(0x1000, 0x1100);
        m.put_ptr(0x1118, 0x1200);
        m.put(0x1240, &1234i32.to_le_bytes());
        m
    }

    #[test]
    fn follows_64_and_32_bit_chains() {
        for size in [8, 4] {
            let m = chain(size);
            assert_eq!(resolve(&m, 0x1000, &[0x18, 0x40]).unwrap(), 0x1240, "punters de {size} bytes");
        }
    }

    #[test]
    fn empty_chain_returns_base_and_negative_offsets_work() {
        let m = chain(8);
        assert_eq!(resolve(&m, 0x1234, &[]).unwrap(), 0x1234);
        assert_eq!(resolve(&m, 0x1000, &[-0x10]).unwrap(), 0x10F0);
    }

    #[test]
    fn null_and_unreadable_pointers_fail() {
        let m = chain(8);
        assert!(matches!(resolve(&m, 0x1300, &[0]), Err(AppError::NullPointer { step: 0, .. })));
        // el 3r pas llegeix el valor 1234 com si fos un punter: el 4t pas ja surt de la memòria
        assert!(matches!(resolve(&m, 0x1000, &[0x18, 0x40, 0, 0]), Err(AppError::ReadFailed { address: 0x4D2, .. })));
        assert!(matches!(resolve(&m, 0x9000, &[0]), Err(AppError::ReadFailed { .. })));
    }
}
