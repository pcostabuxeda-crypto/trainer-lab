//! Catàleg de definicions de trainers.
//!
//! Es llegeixen de dues carpetes: els exemples inclosos amb l'app i la carpeta de l'usuari
//! (`%APPDATA%\cat.trainerlab.app\trainers`). Si dos fitxers tenen el mateix `id`, guanya
//! el de l'usuari. Un fitxer invàlid no tomba res: surt a la llista d'errors.

pub mod engine;
pub mod model;

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use model::TrainerDef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TrainerSource {
    Bundled,
    User,
}

#[derive(Debug, Clone)]
pub struct LoadedTrainer {
    pub def: TrainerDef,
    pub path: PathBuf,
    pub source: TrainerSource,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrainerSummary {
    pub id: String,
    pub name: String,
    pub executable: String,
    pub cheat_count: usize,
    pub online: bool,
    pub source: TrainerSource,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LoadError {
    pub path: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CatalogView {
    pub trainers: Vec<TrainerSummary>,
    pub errors: Vec<LoadError>,
    /// Carpeta on l'usuari pot afegir les seves definicions
    pub user_dir: String,
}

#[derive(Default)]
pub struct Catalog {
    pub trainers: BTreeMap<String, LoadedTrainer>,
    pub errors: Vec<LoadError>,
}

impl Catalog {
    /// Llegeix totes les carpetes, en ordre: les posteriors sobreescriuen les anteriors.
    pub fn load(dirs: &[(PathBuf, TrainerSource)]) -> Self {
        let mut cat = Catalog::default();
        for (dir, source) in dirs {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            let mut files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")))
                .collect();
            files.sort();
            for path in files {
                match load_file(&path) {
                    Ok(def) => {
                        if let Some(prev) = cat.trainers.get(&def.id).filter(|t| t.source == *source) {
                            cat.errors.push(LoadError {
                                path: path.display().to_string(),
                                errors: vec![format!("id: «{}» ja està definit a {}", def.id, prev.path.display())],
                            });
                            continue;
                        }
                        cat.trainers.insert(def.id.clone(), LoadedTrainer { def, path, source: *source });
                    }
                    Err(errors) => cat.errors.push(LoadError { path: path.display().to_string(), errors }),
                }
            }
        }
        cat
    }

    /// Definició per a un executable (sense distingir majúscules).
    pub fn for_executable(&self, exe: &str) -> Option<&LoadedTrainer> {
        self.trainers.values().find(|t| t.def.process.executable.eq_ignore_ascii_case(exe))
    }

    pub fn view(&self, user_dir: &Path) -> CatalogView {
        CatalogView {
            trainers: self
                .trainers
                .values()
                .map(|t| TrainerSummary {
                    id: t.def.id.clone(),
                    name: t.def.name.clone(),
                    executable: t.def.process.executable.clone(),
                    cheat_count: t.def.cheats.len(),
                    online: t.def.online,
                    source: t.source,
                    path: t.path.display().to_string(),
                })
                .collect(),
            errors: self.errors.clone(),
            user_dir: user_dir.display().to_string(),
        }
    }
}

fn load_file(path: &Path) -> Result<TrainerDef, Vec<String>> {
    let text = std::fs::read_to_string(path).map_err(|e| vec![format!("no s'ha pogut llegir: {e}")])?;
    let def: TrainerDef = serde_json::from_str(&text).map_err(|e| vec![format!("JSON: {e}")])?;
    model::validate(&def)?;
    Ok(def)
}

/// Estat compartit de Tauri: el catàleg i les carpetes d'on surt.
pub struct TrainerStore {
    pub catalog: RwLock<Catalog>,
    pub dirs: Vec<(PathBuf, TrainerSource)>,
    pub user_dir: PathBuf,
}

impl TrainerStore {
    pub fn new(bundled_dir: PathBuf, user_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&user_dir);
        let dirs = vec![(bundled_dir, TrainerSource::Bundled), (user_dir.clone(), TrainerSource::User)];
        let store = Self { catalog: RwLock::new(Catalog::load(&dirs)), dirs, user_dir };
        store.log_summary();
        store
    }

    pub fn reload(&self) {
        *self.catalog.write().unwrap_or_else(|e| e.into_inner()) = Catalog::load(&self.dirs);
        self.log_summary();
    }

    pub fn view(&self) -> CatalogView {
        self.catalog.read().unwrap_or_else(|e| e.into_inner()).view(&self.user_dir)
    }

    fn log_summary(&self) {
        let cat = self.catalog.read().unwrap_or_else(|e| e.into_inner());
        tracing::info!(trainers = cat.trainers.len(), errors = cat.errors.len(), "definicions carregades");
        for e in &cat.errors {
            tracing::warn!(path = %e.path, "definició invàlida: {}", e.errors.join(" · "));
        }
    }
}

/// Recàrrega en calent: vigila les carpetes i recarrega quan canvia un `.json`.
/// Els canvis s'agrupen (300 ms) perquè els editors solen escriure en diversos passos.
pub fn watch(app: AppHandle) {
    use notify::{RecursiveMode, Watcher};
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => return tracing::warn!("no es pot vigilar la carpeta de trainers: {e}"),
    };
    for (dir, _) in &app.state::<TrainerStore>().dirs {
        if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            tracing::warn!(dir = %dir.display(), "no es pot vigilar: {e}");
        }
    }
    let _ = std::thread::Builder::new().name("trainers-watch".into()).spawn(move || {
        let _watcher = watcher; // viu mentre el fil viu
        while let Ok(event) = rx.recv() {
            let is_json = |e: &notify::Result<notify::Event>| {
                e.as_ref().is_ok_and(|e| e.paths.iter().any(|p| p.extension().is_some_and(|x| x == "json")))
            };
            if !is_json(&event) {
                continue;
            }
            while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
            app.state::<TrainerStore>().reload();
            let _ = app.emit("trainers-changed", ());
        }
    });
}

/// SHA-256 de l'executable, en hex. Es guarda en memòria per ruta + mida + data, perquè
/// els executables grans tarden a llegir-se.
pub fn exe_sha256(path: &Path) -> std::io::Result<String> {
    /// (ruta, mida, data de modificació) → hash
    type HashCache = HashMap<(PathBuf, u64, SystemTime), String>;
    static CACHE: Mutex<Option<HashCache>> = Mutex::new(None);
    let meta = std::fs::metadata(path)?;
    let key = (path.to_path_buf(), meta.len(), meta.modified()?);
    if let Some(h) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|c| c.get(&key)) {
        return Ok(h.clone());
    }
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_default().insert(key, hex.clone());
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, id: &str, exe: &str) {
        let json = format!(
            r#"{{ "schemaVersion": 1, "id": "{id}", "name": "{name}", "process": {{ "executable": "{exe}" }},
                 "cheats": [{{ "id": "v", "name": "V", "kind": "value", "target": {{ "address": {{ "offset": 0 }}, "type": "i32" }} }}] }}"#
        );
        std::fs::write(dir.join(format!("{name}.json")), json).unwrap();
    }

    #[test]
    fn loads_valid_files_reports_invalid_and_user_overrides_bundled() {
        let (bundled, user) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(bundled.path(), "a", "joc-a", "A.exe");
        write(bundled.path(), "b", "joc-b", "b.exe");
        write(user.path(), "b2", "joc-b", "b.exe");
        std::fs::write(bundled.path().join("trencat.json"), "{ no és json").unwrap();
        std::fs::write(bundled.path().join("llegeix-me.txt"), "ignorat").unwrap();

        let cat = Catalog::load(&[
            (bundled.path().to_path_buf(), TrainerSource::Bundled),
            (user.path().to_path_buf(), TrainerSource::User),
        ]);
        assert_eq!(cat.trainers.len(), 2);
        assert_eq!(cat.trainers["joc-b"].source, TrainerSource::User);
        assert_eq!(cat.errors.len(), 1);
        assert!(cat.errors[0].path.ends_with("trencat.json"));
        assert_eq!(cat.for_executable("a.EXE").unwrap().def.id, "joc-a");
    }

    #[test]
    fn duplicate_ids_in_same_folder_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "x", "mateix", "x.exe");
        write(dir.path(), "y", "mateix", "y.exe");
        let cat = Catalog::load(&[(dir.path().to_path_buf(), TrainerSource::Bundled)]);
        assert_eq!(cat.trainers.len(), 1);
        assert!(cat.errors[0].errors[0].contains("ja està definit"));
    }

    #[test]
    fn hashes_files() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.exe");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(exe_sha256(&f).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
