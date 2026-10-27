//! Sessió = el joc al qual estem adjuntats. Només n'hi ha una alhora.
//!
//! En adjuntar: obrim el procés, llistem els mòduls, comprovem l'anti-cheat i engeguem
//! el fil de congelació i un fil vigilant. El vigilant detecta quan el joc es tanca,
//! neteja la sessió i avisa la UI (`session-state`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::anticheat;
use crate::error::{AppError, AppResult};
use crate::memory::freeze::Freezer;
use crate::memory::process::{self, Arch, Process};
use crate::memory::ModuleInfo;

pub const DEFAULT_FREEZE_INTERVAL_MS: u64 = 100;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionInfo {
    pub pid: u32,
    pub name: String,
    pub arch: Arch,
    /// Adreça base de l'executable, en hex
    pub main_base: String,
    pub module_count: usize,
    /// Anti-cheats del sistema que l'usuari ha acceptat en adjuntar-se
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "status", rename_all = "camelCase")]
#[ts(export)]
pub enum SessionEvent {
    Attached { info: SessionInfo },
    Detached { reason: String },
}

pub struct Session {
    id: u64,
    pub info: SessionInfo,
    pub process: Arc<Process>,
    pub modules: Vec<ModuleInfo>,
    pub freezer: Freezer,
    stop_watch: Arc<AtomicBool>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // El Freezer s'atura sol en el seu Drop; aquí aturem el vigilant
        self.stop_watch.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct SessionState {
    inner: Mutex<Option<Session>>,
    next_id: AtomicU64,
}

impl SessionState {
    pub fn attach(&self, app: &AppHandle, pid: u32, acknowledge_warnings: bool) -> AppResult<SessionInfo> {
        let process = Process::open(pid)?;
        let modules = process.modules()?;

        let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
        let process_names: Vec<String> = process::list_processes()?.into_iter().map(|p| p.name).collect();
        let report = anticheat::check(&module_names, &process_names);
        if !report.in_game.is_empty() {
            let list = report.in_game.join(", ");
            tracing::warn!(pid, anticheat = %list, "adjuntar bloquejat: anti-cheat dins del joc");
            return Err(AppError::AntiCheatBlocked(format!(
                "aquest joc té carregat {list}. Trainer Lab no funciona amb jocs protegits ni en línia."
            )));
        }
        if !report.system.is_empty() && !acknowledge_warnings {
            return Err(AppError::AntiCheatWarning(format!(
                "{} està en marxa al sistema (però no dins d'aquest joc). Continua només si el joc és single-player i offline.",
                report.system.join(", ")
            )));
        }

        let main = modules.first().ok_or_else(|| AppError::ModuleNotFound("(executable principal)".into()))?;
        let info = SessionInfo {
            pid,
            name: process.name.clone(),
            arch: process.arch,
            main_base: format!("{:#x}", main.base),
            module_count: modules.len(),
            warnings: report.system,
        };

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let process = Arc::new(process);
        let stop_watch = Arc::new(AtomicBool::new(false));
        spawn_watcher(app.clone(), id, process.clone(), stop_watch.clone());
        let session = Session {
            id,
            info: info.clone(),
            freezer: Freezer::start(process.clone(), DEFAULT_FREEZE_INTERVAL_MS),
            process,
            modules,
            stop_watch,
        };

        let previous = self.lock().replace(session);
        drop(previous);
        tracing::info!(pid, name = %info.name, arch = ?info.arch, "adjuntat al procés");
        let _ = app.emit("session-state", SessionEvent::Attached { info: info.clone() });
        Ok(info)
    }

    /// Desadjunta si la sessió actual és `only_id` (o qualsevol, si és `None`).
    pub fn detach(&self, app: &AppHandle, reason: &str, only_id: Option<u64>) -> bool {
        let taken = {
            let mut guard = self.lock();
            match (&*guard, only_id) {
                (Some(s), Some(id)) if s.id != id => None,
                _ => guard.take(),
            }
        };
        let Some(session) = taken else { return false };
        tracing::info!(pid = session.info.pid, reason, "desadjuntat");
        drop(session);
        let _ = app.emit("session-state", SessionEvent::Detached { reason: reason.into() });
        true
    }

    pub fn info(&self) -> Option<SessionInfo> {
        self.lock().as_ref().map(|s| s.info.clone())
    }

    pub fn with<T>(&self, f: impl FnOnce(&Session) -> AppResult<T>) -> AppResult<T> {
        let guard = self.lock();
        f(guard.as_ref().ok_or(AppError::NotAttached)?)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn spawn_watcher(app: AppHandle, id: u64, process: Arc<Process>, stop: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new().name("process-watcher".into()).spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if process.wait_exit(250) {
                app.state::<SessionState>().detach(&app, "El joc s'ha tancat", Some(id));
                return;
            }
        }
    });
}
