//! Sessió = el joc al qual estem adjuntats. Només n'hi ha una alhora.
//!
//! En adjuntar: obrim el procés, llistem els mòduls, comprovem l'anti-cheat, busquem la
//! definició del joc (si n'hi ha), comprovem la versió i engeguem el fil de congelació, un
//! fil vigilant i les dreceres. El vigilant detecta quan el joc es tanca, neteja la sessió
//! i avisa la UI (`session-state`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::anticheat;
use crate::error::{AppError, AppResult};
use crate::hotkeys;
use crate::memory::ModuleInfo;
use crate::memory::freeze::Freezer;
use crate::memory::process::{self, Arch, Process};
use crate::settings::SettingsStore;
use crate::trainers::engine::{ActiveTrainer, VersionInfo};
use crate::trainers::{TrainerStore, exe_sha256};

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrainerRef {
    pub id: String,
    pub name: String,
    pub version: VersionInfo,
}

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
    /// Avisos d'anti-cheat que l'usuari ha acceptat en adjuntar-se
    pub warnings: Vec<String>,
    /// Definició trobada per a aquest executable
    pub trainer: Option<TrainerRef>,
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
    pub trainer: Mutex<Option<ActiveTrainer>>,
    /// Dreceres que no s'han pogut registrar: (clau, motiu)
    pub hotkey_errors: Mutex<Vec<(String, String)>>,
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

        // 1. Anti-cheat dins del joc: bloqueja
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

        // 2. Definició del joc: `online` bloqueja; l'arquitectura ha de coincidir
        let def = app.state::<TrainerStore>().catalog.read().unwrap_or_else(|e| e.into_inner()).for_executable(&process.name).map(|t| t.def.clone());
        if let Some(def) = &def {
            if def.online {
                return Err(AppError::AntiCheatBlocked(format!("«{}» està marcat com a joc en línia.", def.name)));
            }
            if let Some(arch) = def.process.arch
                && arch != process.arch
            {
                return Err(AppError::InvalidValue(format!(
                    "la definició «{}» és per a {arch:?}, però el joc és {:?}.",
                    def.name, process.arch
                )));
            }
        }

        // 3. Anti-cheat al sistema o a la carpeta del joc: cal confirmació
        let mut warnings = report.system;
        let install_dir = std::path::Path::new(&process.path).parent();
        let entries: Vec<String> = install_dir
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        warnings.extend(anticheat::check_install_dir(&entries));
        if !warnings.is_empty() && !acknowledge_warnings {
            return Err(AppError::AntiCheatWarning(format!(
                "s'ha detectat {}, però no dins d'aquest joc. Continua només si el joc és single-player i el fas servir offline.",
                warnings.join(", ")
            )));
        }

        // 4. Versió i trucs
        let trainer = def.map(|def| {
            let sha = exe_sha256(std::path::Path::new(&process.path)).unwrap_or_else(|e| {
                tracing::warn!("no s'ha pogut calcular l'SHA-256 de l'executable: {e}");
                String::new()
            });
            let version = VersionInfo::check(&def, sha);
            ActiveTrainer::new(def, version, &process, &modules)
        });

        let main = modules.first().ok_or_else(|| AppError::ModuleNotFound("(executable principal)".into()))?;
        let info = SessionInfo {
            pid,
            name: process.name.clone(),
            arch: process.arch,
            main_base: format!("{:#x}", main.base),
            module_count: modules.len(),
            warnings,
            trainer: trainer
                .as_ref()
                .map(|t| TrainerRef { id: t.def.id.clone(), name: t.def.name.clone(), version: t.version.clone() }),
        };

        // Si ja hi havia una sessió, es tanca bé (restaurant els trucs) abans de la nova
        self.detach(app, "S'ha adjuntat a un altre procés", None, true);

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let process = Arc::new(process);
        let stop_watch = Arc::new(AtomicBool::new(false));
        spawn_watcher(app.clone(), id, process.clone(), stop_watch.clone());
        let interval = app.state::<SettingsStore>().get().freeze_interval_ms;
        let hotkey_def = trainer.as_ref().map(|t| t.def.clone());
        let session = Session {
            id,
            info: info.clone(),
            freezer: Freezer::start(process.clone(), interval),
            process,
            modules,
            trainer: Mutex::new(trainer),
            hotkey_errors: Mutex::new(Vec::new()),
            stop_watch,
        };
        *self.lock() = Some(session);
        if let Some(def) = hotkey_def {
            self.set_hotkey_errors(hotkeys::register(app, &def));
        }

        tracing::info!(pid, name = %info.name, arch = ?info.arch, trainer = ?info.trainer.as_ref().map(|t| &t.id), "adjuntat al procés");
        let _ = app.emit("session-state", SessionEvent::Attached { info: info.clone() });
        Ok(info)
    }

    /// Desadjunta si la sessió actual és `only_id` (o qualsevol, si és `None`). Amb
    /// `restore`, desactiva els trucs i torna a posar els valors i bytes originals.
    pub fn detach(&self, app: &AppHandle, reason: &str, only_id: Option<u64>, restore: bool) -> bool {
        let taken = {
            let mut guard = self.lock();
            match (&*guard, only_id) {
                (Some(s), Some(id)) if s.id != id => None,
                _ => guard.take(),
            }
        };
        let Some(session) = taken else { return false };
        hotkeys::unregister_all(app);
        if restore && let Some(t) = session.trainer.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            t.deactivate_all(session.process.as_ref(), &session.freezer);
        }
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

    pub fn with_trainer<T>(&self, f: impl FnOnce(&Session, &mut ActiveTrainer) -> AppResult<T>) -> AppResult<T> {
        self.with(|s| {
            let mut t = s.trainer.lock().unwrap_or_else(|e| e.into_inner());
            let t = t.as_mut().ok_or_else(|| AppError::InvalidValue("aquest joc no té cap definició de trainer".into()))?;
            f(s, t)
        })
    }

    pub fn set_hotkey_errors(&self, errors: Vec<(String, String)>) {
        if let Some(s) = self.lock().as_ref() {
            *s.hotkey_errors.lock().unwrap_or_else(|e| e.into_inner()) = errors;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn spawn_watcher(app: AppHandle, id: u64, process: Arc<Process>, stop: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new().name("process-watcher".into()).spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if process.wait_exit(250) {
                // El joc ja no hi és: no hi ha res a restaurar
                app.state::<SessionState>().detach(&app, "El joc s'ha tancat", Some(id), false);
                return;
            }
        }
    });
}
