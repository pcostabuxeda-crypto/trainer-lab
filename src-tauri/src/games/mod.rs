//! Biblioteca de jocs: jocs instal·lats (Steam, Epic, GOG i afegits a mà), quina definició
//! de trainer els correspon, i quins estan en execució ara mateix.

mod epic;
mod gog;
pub mod registry;
mod steam;
mod vdf;

use std::collections::HashSet;
use std::path::Path;
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::memory::process::{self, ProcessInfo};
use crate::settings::SettingsStore;
use crate::trainers::{Catalog, TrainerStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Store {
    Steam,
    Epic,
    Gog,
    Manual,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameEntry {
    /// `botiga:id`, p. ex. `steam:620`
    pub id: String,
    pub name: String,
    pub store: Store,
    pub store_id: String,
    pub install_dir: String,
    /// Executable principal, si la botiga el diu
    pub exe: Option<String>,
    /// Definició de trainer que li correspon
    pub trainer_id: Option<String>,
}

impl GameEntry {
    pub fn new(store: Store, store_id: &str, name: &str, install_dir: &Path, exe: Option<&Path>) -> Self {
        let prefix = match store {
            Store::Steam => "steam",
            Store::Epic => "epic",
            Store::Gog => "gog",
            Store::Manual => "manual",
        };
        Self {
            id: format!("{prefix}:{store_id}"),
            name: name.into(),
            store,
            store_id: store_id.into(),
            install_dir: install_dir.display().to_string(),
            exe: exe.map(|e| e.display().to_string()),
            trainer_id: None,
        }
    }
}

/// Un procés en marxa que és un joc conegut o té trainer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunningGame {
    pub pid: u32,
    pub process_name: String,
    /// Nom del joc (de la botiga o de la definició)
    pub name: String,
    pub game_id: Option<String>,
    pub trainer_id: Option<String>,
}

/// Processos que mai són "el joc", encara que siguin a la seva carpeta.
const HELPERS: &[&str] = &["crash", "unins", "redist", "setup", "report", "launcher_helper", "easyanticheat", "be_service"];

fn scan_installed(app: &AppHandle) -> Vec<GameEntry> {
    let mut games = Vec::new();
    if let Some(root) = steam::root() {
        games.extend(steam::scan(&root));
    }
    if let Some(dir) = epic::manifests_dir() {
        games.extend(epic::scan(&dir));
    }
    games.extend(gog::scan());
    for m in app.state::<SettingsStore>().get().manual_games {
        let path = Path::new(&m.path);
        let dir = path.parent().unwrap_or(path);
        games.push(GameEntry::new(Store::Manual, &m.path.to_lowercase(), &m.name, dir, Some(path)));
    }
    games.sort_by_cached_key(|g| g.name.to_lowercase());
    games
}

/// Assigna a cada joc la definició que li correspon:
/// 1. pels identificadors de botiga del camp `match` de la definició;
/// 2. per l'executable principal que diu la botiga;
/// 3. buscant l'executable de la definició dins la carpeta del joc (fins a 4 nivells).
pub fn match_trainers(games: &mut [GameEntry], catalog: &Catalog) {
    for g in games.iter_mut() {
        g.trainer_id = catalog
            .trainers
            .values()
            .find(|t| {
                let ids = t.def.store_ids.as_ref();
                match g.store {
                    Store::Steam => ids.and_then(|i| i.steam_app_id).is_some_and(|id| id.to_string() == g.store_id),
                    Store::Epic => ids.and_then(|i| i.epic_app_name.as_deref()) == Some(g.store_id.as_str()),
                    Store::Gog => ids.and_then(|i| i.gog_id.as_deref()) == Some(g.store_id.as_str()),
                    Store::Manual => false,
                }
            })
            .or_else(|| {
                let exe_name = g.exe.as_deref().and_then(|e| Path::new(e).file_name()).map(|n| n.to_string_lossy());
                exe_name.and_then(|n| catalog.for_executable(&n))
            })
            .or_else(|| {
                let names = exe_names_in(Path::new(&g.install_dir), 4, 4000);
                catalog.trainers.values().find(|t| names.contains(&t.def.process.executable.to_lowercase()))
            })
            .map(|t| t.def.id.clone());
    }
}

/// Noms (en minúscules) dels `.exe` dins `dir`, amb límit de profunditat i d'entrades
/// perquè un joc enorme no bloquegi l'escaneig.
fn exe_names_in(dir: &Path, max_depth: usize, max_entries: usize) -> HashSet<String> {
    let mut found = HashSet::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    let mut seen = 0;
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            seen += 1;
            if seen > max_entries {
                return found;
            }
            let Ok(ft) = e.file_type() else { continue };
            let name = e.file_name().to_string_lossy().to_lowercase();
            if ft.is_dir() && depth < max_depth {
                stack.push((e.path(), depth + 1));
            } else if ft.is_file() && name.ends_with(".exe") {
                found.insert(name);
            }
        }
    }
    found
}

/// Quins processos són jocs: tenen definició o s'executen des de la carpeta d'un joc.
pub fn find_running(processes: &[ProcessInfo], games: &[GameEntry], catalog: &Catalog) -> Vec<RunningGame> {
    let dirs: Vec<(String, &GameEntry)> =
        games.iter().map(|g| (format!("{}\\", g.install_dir.trim_end_matches('\\').to_lowercase()), g)).collect();
    let mut out: Vec<RunningGame> = processes
        .iter()
        .filter_map(|p| {
            let lname = p.name.to_lowercase();
            let trainer = catalog.for_executable(&p.name);
            let game = p.path.as_ref().and_then(|path| {
                let path = path.to_lowercase();
                dirs.iter().find(|(d, _)| path.starts_with(d)).map(|(_, g)| *g)
            });
            if trainer.is_none() && (game.is_none() || HELPERS.iter().any(|h| lname.contains(h))) {
                return None;
            }
            Some(RunningGame {
                pid: p.pid,
                process_name: p.name.clone(),
                name: game.map(|g| g.name.clone()).or_else(|| trainer.map(|t| t.def.name.clone())).unwrap_or_default(),
                game_id: game.map(|g| g.id.clone()),
                trainer_id: trainer.map(|t| t.def.id.clone()),
            })
        })
        .collect();
    // Primer els que tenen trainer
    out.sort_by_key(|r| (r.trainer_id.is_none(), r.name.to_lowercase(), r.pid));
    out
}

/// Estat compartit: jocs instal·lats (en memòria cau) i jocs en execució.
#[derive(Default)]
pub struct GameStore {
    installed: RwLock<Option<Vec<GameEntry>>>,
    running: RwLock<Vec<RunningGame>>,
}

impl GameStore {
    /// Jocs instal·lats. Es busquen la primera vegada o quan `refresh` és cert.
    pub fn installed(&self, app: &AppHandle, refresh: bool) -> Vec<GameEntry> {
        if !refresh && let Some(g) = self.installed.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return g.clone();
        }
        let started = std::time::Instant::now();
        let mut games = scan_installed(app);
        match_trainers(&mut games, &app.state::<TrainerStore>().catalog.read().unwrap_or_else(|e| e.into_inner()));
        tracing::info!(count = games.len(), ms = started.elapsed().as_millis() as u64, "biblioteca escanejada");
        *self.installed.write().unwrap_or_else(|e| e.into_inner()) = Some(games.clone());
        games
    }

    /// Torna a assignar trainers sense tornar a escanejar (quan canvien les definicions).
    pub fn rematch(&self, catalog: &Catalog) {
        if let Some(g) = self.installed.write().unwrap_or_else(|e| e.into_inner()).as_mut() {
            match_trainers(g, catalog);
        }
    }

    pub fn running(&self) -> Vec<RunningGame> {
        self.running.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Fil que cada 2 s mira quins jocs estan en marxa i avisa la UI (`running-games`) si canvia.
pub fn watch_running(app: AppHandle) {
    let _ = std::thread::Builder::new().name("running-games".into()).spawn(move || {
        let store = app.state::<GameStore>();
        let games = store.installed(&app, false);
        loop {
            if let Ok(procs) = process::list_processes() {
                let installed = store.installed.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| games.clone());
                let catalog = app.state::<TrainerStore>();
                let now = find_running(&procs, &installed, &catalog.catalog.read().unwrap_or_else(|e| e.into_inner()));
                let changed = *store.running.read().unwrap_or_else(|e| e.into_inner()) != now;
                if changed {
                    tracing::debug!(count = now.len(), "jocs en execució");
                    *store.running.write().unwrap_or_else(|e| e.into_inner()) = now.clone();
                    let _ = app.emit("running-games", now);
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::memory::process::Arch;
    use crate::trainers::TrainerSource;

    fn catalog(defs: &[(&str, &str, &str)]) -> Catalog {
        let dir = tempfile::tempdir().unwrap();
        for (id, exe, extra) in defs {
            let json = format!(
                r#"{{ "schemaVersion": 1, "id": "{id}", "name": "{id}", "process": {{ "executable": "{exe}" }} {extra},
                     "cheats": [{{ "id": "v", "name": "V", "kind": "value", "target": {{ "address": {{ "offset": 0 }}, "type": "i32" }} }}] }}"#
            );
            std::fs::write(dir.path().join(format!("{id}.json")), json).unwrap();
        }
        let cat = Catalog::load(&[(PathBuf::from(dir.path()), TrainerSource::Bundled)]);
        assert!(cat.errors.is_empty(), "{:?}", cat.errors);
        cat
    }

    fn proc(pid: u32, path: &str) -> ProcessInfo {
        ProcessInfo { pid, name: path.rsplit('\\').next().unwrap().into(), path: Some(path.into()), arch: Arch::X64 }
    }

    #[test]
    fn matches_trainers_by_store_id_exe_and_folder_search() {
        let root = tempfile::tempdir().unwrap();
        let deep = root.path().join("C/Binaries/Win64");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("GameC.exe"), b"").unwrap();
        let cat = catalog(&[
            ("a", "a.exe", r#", "match": { "steamAppId": 620 }"#),
            ("b", "b.exe", ""),
            ("c", "gamec.exe", ""),
        ]);
        let mut games = vec![
            GameEntry::new(Store::Steam, "620", "A", root.path(), None),
            GameEntry::new(Store::Epic, "B", "B", root.path(), Some(Path::new(r"X:\B\B.exe"))),
            GameEntry::new(Store::Steam, "1", "C", &root.path().join("C"), None),
            GameEntry::new(Store::Steam, "2", "Sense", &root.path().join("buit"), None),
        ];
        match_trainers(&mut games, &cat);
        let ids: Vec<_> = games.iter().map(|g| g.trainer_id.as_deref()).collect();
        assert_eq!(ids, [Some("a"), Some("b"), Some("c"), None]);
    }

    #[test]
    fn finds_running_games_and_ignores_helpers() {
        let cat = catalog(&[("t", "test-game.exe", "")]);
        let games = vec![GameEntry::new(Store::Steam, "9", "Joc Gran", Path::new(r"D:\Steam\common\Joc Gran"), None)];
        let procs = [
            proc(10, r"C:\Windows\explorer.exe"),
            proc(11, r"D:\Steam\common\Joc Gran\Bin\JocGran.exe"),
            proc(12, r"D:\Steam\common\Joc Gran\UnityCrashHandler64.exe"),
            proc(13, r"C:\dev\test-game.exe"),
            proc(14, r"D:\Steam\common\Joc Gran Dos\altre.exe"), // carpeta diferent amb el mateix prefix
        ];
        let r = find_running(&procs, &games, &cat);
        let got: Vec<_> = r.iter().map(|g| (g.pid, g.name.as_str(), g.trainer_id.as_deref())).collect();
        assert_eq!(got, [(13, "t", Some("t")), (11, "Joc Gran", None)]);
    }

    /// Escaneig real d'aquest PC: `cargo test -p trainer-lab --lib scans_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scans_this_pc() {
        let started = std::time::Instant::now();
        let mut all = Vec::new();
        if let Some(root) = steam::root() {
            all.extend(steam::scan(&root));
        }
        if let Some(dir) = epic::manifests_dir() {
            all.extend(epic::scan(&dir));
        }
        all.extend(gog::scan());
        for g in &all {
            println!("{:<6?} {:<45} {}", g.store, g.name, g.install_dir);
        }
        println!("{} jocs en {:?}", all.len(), started.elapsed());
        assert!(!all.is_empty());
    }

    #[test]
    fn exe_search_respects_limits() {
        let root = tempfile::tempdir().unwrap();
        let deep = root.path().join("1/2/3/4/5");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("massa.exe"), b"").unwrap();
        std::fs::write(root.path().join("1/aqui.exe"), b"").unwrap();
        let names = exe_names_in(root.path(), 4, 1000);
        assert!(names.contains("aqui.exe"));
        assert!(!names.contains("massa.exe"), "massa profund");
    }
}
