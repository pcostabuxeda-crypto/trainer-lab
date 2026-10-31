//! Memòria simulada per als tests: un sol bloc de bytes que comença a `base`.

use std::sync::Mutex;

use super::{Memory, Region, WriteMode, check_write};
use crate::error::{AppError, AppResult};

pub struct FakeMemory {
    pub base: u64,
    pub data: Mutex<Vec<u8>>,
    pub ptr_size: usize,
    pub writable: bool,
    pub mapped: bool,
}

impl FakeMemory {
    pub fn new(base: u64, size: usize, ptr_size: usize) -> Self {
        Self { base, data: Mutex::new(vec![0; size]), ptr_size, writable: true, mapped: false }
    }

    pub fn put(&self, address: u64, bytes: &[u8]) {
        let off = (address - self.base) as usize;
        self.data.lock().unwrap()[off..off + bytes.len()].copy_from_slice(bytes);
    }

    pub fn put_ptr(&self, address: u64, value: u64) {
        self.put(address, &value.to_le_bytes()[..self.ptr_size]);
    }
}

impl Memory for FakeMemory {
    fn read_bytes(&self, address: u64, buf: &mut [u8]) -> AppResult<()> {
        let data = self.data.lock().unwrap();
        let fail = AppError::ReadFailed { address, len: buf.len() };
        let off = address.checked_sub(self.base).ok_or(fail)? as usize;
        let src = data.get(off..off + buf.len()).ok_or(AppError::ReadFailed { address, len: buf.len() })?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_bytes(&self, address: u64, bytes: &[u8], mode: WriteMode) -> AppResult<()> {
        check_write(self.region(address), address, bytes.len(), mode)?;
        self.put(address, bytes);
        Ok(())
    }

    fn pointer_size(&self) -> usize {
        self.ptr_size
    }

    fn region(&self, address: u64) -> Option<Region> {
        let size = self.data.lock().unwrap().len() as u64;
        (address >= self.base && address < self.base + size).then_some(Region {
            base: self.base,
            size,
            committed: true,
            readable: true,
            writable: self.writable,
            executable: false,
            guard: false,
            mapped: self.mapped,
        })
    }
}
