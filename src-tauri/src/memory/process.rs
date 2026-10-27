//! Processos de Windows: llistar-los, obrir-ne un i llegir/escriure la seva memòria.
//!
//! Llistar (no cal obrir res amb permisos d'escriptura):
//! 1. `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` fa una "foto" de tots els processos.
//! 2. `Process32FirstW` / `Process32NextW` la recorren: PID i nom de l'executable.
//! 3. Per a la ruta i l'arquitectura obrim cada procés amb `PROCESS_QUERY_LIMITED_INFORMATION`,
//!    que no dona accés a la memòria. Els processos protegits el deneguen i surten sense ruta.
//!
//! Adjuntar-se ([`Process::open`]): demanem només el que cal per llegir i escriure memòria
//! (`PROCESS_VM_*`), consultar-lo i saber quan es tanca (`SYNCHRONIZE`). Per a jocs del mateix
//! usuari no cal administrador.

use std::ffi::c_void;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use windows::Win32::Foundation::{CloseHandle, ERROR_BAD_LENGTH, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::Debug::{FlushInstructionCache, ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, PROCESSENTRY32W, Process32FirstW,
    Process32NextW, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtectEx,
    VirtualQueryEx,
};
use windows::Win32::System::SystemInformation::{
    IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386,
    IMAGE_FILE_MACHINE_UNKNOWN,
};
use windows::Win32::System::Threading::{
    IsWow64Process2, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE, QueryFullProcessImageNameW, WaitForSingleObject,
};
use windows::core::PWSTR;

use super::{Memory, ModuleInfo, Region, WriteMode, check_write};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Arch {
    X64,
    X86,
    Arm64,
    Unknown,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    /// `None` si Windows no ens deixa consultar el procés (processos protegits)
    pub path: Option<String>,
    pub arch: Arch,
}

/// Handle de Windows que es tanca sol quan surt d'àmbit (evita fuites de handles).
pub struct OwnedHandle(pub HANDLE);

// Un handle de procés del kernel es pot fer servir des de qualsevol fil.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Tots els processos visibles, ordenats per nom (sense distingir majúscules).
pub fn list_processes() -> AppResult<Vec<ProcessInfo>> {
    let snapshot = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut out = Vec::new();
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while ok {
        let pid = entry.th32ProcessID;
        // PID 0 (Idle) i 4 (System) no són processos d'usuari
        if pid > 4 {
            let (path, arch) = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
                Ok(h) => {
                    let h = OwnedHandle(h);
                    (image_path(h.0), arch_of(h.0))
                }
                Err(_) => (None, Arch::Unknown),
            };
            out.push(ProcessInfo { pid, name: wide_to_string(&entry.szExeFile), path, arch });
        }
        ok = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    out.sort_by_cached_key(|p| (p.name.to_lowercase(), p.pid));
    Ok(out)
}

/// Un procés obert per llegir-ne i escriure-ne la memòria.
pub struct Process {
    handle: OwnedHandle,
    pub pid: u32,
    pub name: String,
    /// Ruta completa de l'executable
    pub path: String,
    pub arch: Arch,
}

impl Process {
    pub fn open(pid: u32) -> AppResult<Self> {
        let access = PROCESS_VM_READ
            | PROCESS_VM_WRITE
            | PROCESS_VM_OPERATION
            | PROCESS_QUERY_LIMITED_INFORMATION
            | PROCESS_SYNCHRONIZE;
        let handle = OwnedHandle(unsafe { OpenProcess(access, false, pid)? });
        let path = image_path(handle.0).unwrap_or_default();
        let name = path.rsplit('\\').next().unwrap_or_default().to_string();
        let arch = arch_of(handle.0);
        Ok(Self { handle, pid, name, path, arch })
    }

    /// Mòduls carregats (l'executable i les DLL). El primer sempre és l'executable.
    pub fn modules(&self) -> AppResult<Vec<ModuleInfo>> {
        // SNAPMODULE32 inclou els mòduls de 32 bits d'un joc x86 (WoW64)
        let flags = TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32;
        // Si el joc està carregant DLL en aquell moment, Windows pot retornar ERROR_BAD_LENGTH: es reintenta
        let mut attempt = 0;
        let snapshot = loop {
            match unsafe { CreateToolhelp32Snapshot(flags, self.pid) } {
                Ok(h) => break OwnedHandle(h),
                Err(e) if e.code() == ERROR_BAD_LENGTH.to_hresult() && attempt < 5 => attempt += 1,
                Err(e) => return Err(e.into()),
            }
        };
        let mut entry = MODULEENTRY32W { dwSize: size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
        let mut out = Vec::new();
        let mut ok = unsafe { Module32FirstW(snapshot.0, &mut entry) }.is_ok();
        while ok {
            out.push(ModuleInfo {
                name: wide_to_string(&entry.szModule),
                base: entry.modBaseAddr as u64,
                size: entry.modBaseSize as u64,
            });
            ok = unsafe { Module32NextW(snapshot.0, &mut entry) }.is_ok();
        }
        Ok(out)
    }

    /// Espera fins a `timeout_ms` i diu si el procés ja s'ha tancat.
    pub fn wait_exit(&self, timeout_ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.handle.0, timeout_ms) == WAIT_OBJECT_0 }
    }

    fn write_raw(&self, address: u64, data: &[u8]) -> AppResult<()> {
        let mut written = 0usize;
        unsafe {
            WriteProcessMemory(
                self.handle.0,
                address as *const c_void,
                data.as_ptr() as *const c_void,
                data.len(),
                Some(&mut written),
            )?
        };
        if written != data.len() {
            return Err(AppError::WriteRejected { address, reason: format!("només s'han escrit {written} bytes") });
        }
        Ok(())
    }
}

impl Memory for Process {
    fn read_bytes(&self, address: u64, buf: &mut [u8]) -> AppResult<()> {
        let mut read = 0usize;
        // ReadProcessMemory copia bytes de l'espai d'adreces del joc al nostre buffer
        let r = unsafe {
            ReadProcessMemory(
                self.handle.0,
                address as *const c_void,
                buf.as_mut_ptr() as *mut c_void,
                buf.len(),
                Some(&mut read),
            )
        };
        // Si falla perquè el joc s'ha tancat, ho diem clar; si no, és una adreça invàlida
        // (normalment ERROR_PARTIAL_COPY: part del rang no està mapejat)
        match r {
            Ok(()) if read == buf.len() => Ok(()),
            _ if self.wait_exit(0) => Err(AppError::ProcessExited),
            _ => Err(AppError::ReadFailed { address, len: buf.len() }),
        }
    }

    fn write_bytes(&self, address: u64, data: &[u8], mode: WriteMode) -> AppResult<()> {
        check_write(self.region(address), address, data.len(), mode)?;
        if mode == WriteMode::Data {
            return self.write_raw(address, data);
        }
        // Codi: les pàgines de codi són de només lectura. Les fem escrivibles un moment,
        // escrivim, restaurem la protecció original i buidem la cache d'instruccions.
        let h = self.handle.0;
        let mut old = PAGE_PROTECTION_FLAGS::default();
        unsafe { VirtualProtectEx(h, address as *const c_void, data.len(), PAGE_EXECUTE_READWRITE, &mut old)? };
        let result = self.write_raw(address, data);
        let mut ignored = PAGE_PROTECTION_FLAGS::default();
        unsafe {
            let _ = VirtualProtectEx(h, address as *const c_void, data.len(), old, &mut ignored);
            let _ = FlushInstructionCache(h, Some(address as *const c_void), data.len());
        }
        result
    }

    fn pointer_size(&self) -> usize {
        if self.arch == Arch::X86 { 4 } else { 8 }
    }

    fn region(&self, address: u64) -> Option<Region> {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let n = unsafe {
            VirtualQueryEx(self.handle.0, Some(address as *const c_void), &mut mbi, size_of::<MEMORY_BASIC_INFORMATION>())
        };
        if n == 0 {
            return None;
        }
        let (readable, writable, executable, guard) = protection(mbi.Protect.0);
        Some(Region {
            base: mbi.BaseAddress as u64,
            size: mbi.RegionSize as u64,
            committed: mbi.State == MEM_COMMIT,
            readable,
            writable,
            executable,
            guard,
        })
    }
}

/// Descompon les constants `PAGE_*` de Windows en (llegible, escrivible, executable, guarda).
fn protection(raw: u32) -> (bool, bool, bool, bool) {
    let guard = raw & 0x100 != 0; // PAGE_GUARD
    match raw & 0xFF {
        0x02 => (true, false, false, guard),  // PAGE_READONLY
        0x04 => (true, true, false, guard),   // PAGE_READWRITE
        0x08 => (true, true, false, guard),   // PAGE_WRITECOPY
        0x10 => (false, false, true, guard),  // PAGE_EXECUTE
        0x20 => (true, false, true, guard),   // PAGE_EXECUTE_READ
        0x40 => (true, true, true, guard),    // PAGE_EXECUTE_READWRITE
        0x80 => (true, true, true, guard),    // PAGE_EXECUTE_WRITECOPY
        _ => (false, false, false, guard),    // PAGE_NOACCESS o desconegut
    }
}

fn image_path(h: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }
        .ok()
        .map(|_| String::from_utf16_lossy(&buf[..len as usize]))
}

fn arch_of(h: HANDLE) -> Arch {
    let mut process = IMAGE_FILE_MACHINE::default();
    let mut native = IMAGE_FILE_MACHINE::default();
    match unsafe { IsWow64Process2(h, &mut process, Some(&mut native)) } {
        Ok(()) => machine_to_arch(process, native),
        Err(_) => Arch::Unknown,
    }
}

/// `IsWow64Process2` retorna UNKNOWN per al `process` quan NO és un procés emulat
/// (WoW64): llavors té l'arquitectura nativa del sistema.
fn machine_to_arch(process: IMAGE_FILE_MACHINE, native: IMAGE_FILE_MACHINE) -> Arch {
    let m = if process == IMAGE_FILE_MACHINE_UNKNOWN { native } else { process };
    match m {
        IMAGE_FILE_MACHINE_AMD64 => Arch::X64,
        IMAGE_FILE_MACHINE_I386 => Arch::X86,
        IMAGE_FILE_MACHINE_ARM64 => Arch::Arm64,
        _ => Arch::Unknown,
    }
}

fn wide_to_string(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_mapping() {
        assert_eq!(machine_to_arch(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_AMD64), Arch::X64);
        assert_eq!(machine_to_arch(IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_AMD64), Arch::X86);
        assert_eq!(machine_to_arch(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_ARM64), Arch::Arm64);
    }

    #[test]
    fn protection_flags() {
        assert_eq!(protection(0x04), (true, true, false, false));
        assert_eq!(protection(0x20), (true, false, true, false));
        assert_eq!(protection(0x104), (true, true, false, true));
        assert_eq!(protection(0x01), (false, false, false, false));
    }

    #[test]
    fn finds_current_process_as_x64() {
        let me = std::process::id();
        let list = list_processes().unwrap();
        let p = list.iter().find(|p| p.pid == me).expect("el procés de test ha de sortir a la llista");
        assert_eq!(p.arch, Arch::X64);
        assert!(p.path.as_deref().is_some_and(|s| s.ends_with(".exe")));
    }

    #[test]
    fn reads_and_writes_own_memory() {
        // Ens obrim a nosaltres mateixos: és un procés real i no cal cap joc
        let value = Box::new(1234i32);
        let addr = &*value as *const i32 as u64;
        let p = Process::open(std::process::id()).unwrap();
        let mut b = [0u8; 4];
        p.read_bytes(addr, &mut b).unwrap();
        assert_eq!(i32::from_le_bytes(b), 1234);
        p.write_bytes(addr, &99i32.to_le_bytes(), WriteMode::Data).unwrap();
        assert_eq!(unsafe { std::ptr::read_volatile(addr as *const i32) }, 99);
        assert!(!p.modules().unwrap().is_empty());
        assert!(!p.wait_exit(0));
    }
}
