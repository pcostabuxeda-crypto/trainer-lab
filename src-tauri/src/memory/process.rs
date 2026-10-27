//! Llistat de processos i informació bàsica de cadascun.
//!
//! Com funciona a Windows:
//! 1. `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` fa una "foto" de tots els processos.
//! 2. `Process32FirstW` / `Process32NextW` recorren la foto: PID i nom de l'executable.
//! 3. Per saber la ruta i l'arquitectura cal obrir el procés amb el permís mínim
//!    `PROCESS_QUERY_LIMITED_INFORMATION`. No dona accés a la memòria, només a dades
//!    bàsiques, i funciona sense administrador per als processos del mateix usuari.
//!    Els processos protegits del sistema el deneguen: els llistem igualment, sense ruta.

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
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

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::SystemInformation::{
        IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386,
        IMAGE_FILE_MACHINE_UNKNOWN,
    };
    use windows::Win32::System::Threading::{
        IsWow64Process2, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::core::PWSTR;

    use super::{Arch, ProcessInfo};
    use crate::error::AppResult;

    /// Handle de Windows que es tanca sol quan surt d'àmbit (evita fuites de handles).
    pub struct OwnedHandle(pub HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn list() -> AppResult<Vec<ProcessInfo>> {
        let snapshot = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut out = Vec::new();
        let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
        while ok {
            let pid = entry.th32ProcessID;
            // PID 0 (Idle) i 4 (System) no són processos d'usuari
            if pid > 4 {
                let name = wide_to_string(&entry.szExeFile);
                let (path, arch) = query(pid);
                out.push(ProcessInfo { pid, name, path, arch });
            }
            ok = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
        }
        Ok(out)
    }

    fn query(pid: u32) -> (Option<String>, Arch) {
        let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
            return (None, Arch::Unknown);
        };
        let h = OwnedHandle(h);

        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let path = unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }
            .ok()
            .map(|_| String::from_utf16_lossy(&buf[..len as usize]));

        let mut process = IMAGE_FILE_MACHINE::default();
        let mut native = IMAGE_FILE_MACHINE::default();
        let arch = match unsafe { IsWow64Process2(h.0, &mut process, Some(&mut native)) } {
            Ok(()) => machine_to_arch(process, native),
            Err(_) => Arch::Unknown,
        };
        (path, arch)
    }

    /// `IsWow64Process2` retorna UNKNOWN per al `process` quan NO és un procés emulat
    /// (WoW64): llavors té l'arquitectura nativa del sistema.
    pub fn machine_to_arch(process: IMAGE_FILE_MACHINE, native: IMAGE_FILE_MACHINE) -> Arch {
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
}

/// Tots els processos visibles, ordenats per nom (sense distingir majúscules).
pub fn list_processes() -> AppResult<Vec<ProcessInfo>> {
    #[cfg(windows)]
    let mut list = win::list()?;
    #[cfg(not(windows))]
    let mut list: Vec<ProcessInfo> = Vec::new();
    list.sort_by_cached_key(|p| (p.name.to_lowercase(), p.pid));
    Ok(list)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows::Win32::System::SystemInformation::*;

    #[test]
    fn arch_mapping() {
        use win::machine_to_arch as m;
        assert_eq!(m(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_AMD64), Arch::X64);
        assert_eq!(m(IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_AMD64), Arch::X86);
        assert_eq!(m(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_ARM64), Arch::Arm64);
    }

    #[test]
    fn finds_current_process_as_x64() {
        let me = std::process::id();
        let list = list_processes().unwrap();
        let p = list.iter().find(|p| p.pid == me).expect("el procés de test ha de sortir a la llista");
        assert_eq!(p.arch, Arch::X64);
        assert!(p.path.as_deref().is_some_and(|s| s.ends_with(".exe")));
    }
}
