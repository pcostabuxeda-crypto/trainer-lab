//! Configuració de l'usuari a `%APPDATA%\cat.trainerlab.app\settings.json`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    /// Cada quants mil·lisegons es reescriuen els valors congelats
    #[ts(type = "number")]
    pub freeze_interval_ms: u64,
    /// trainerId → clau → drecera. Clau = id del truc, o `id:increase` / `id:decrease` als
    /// sliders. Una drecera buida (`""`) desactiva la de la definició.
    pub hotkey_overrides: BTreeMap<String, BTreeMap<String, String>>,
    /// Jocs afegits a mà a la Biblioteca
    pub manual_games: Vec<ManualGame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ManualGame {
    pub name: String,
    /// Ruta completa de l'executable
    pub path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { freeze_interval_ms: 100, hotkey_overrides: BTreeMap::new(), manual_games: Vec::new() }
    }
}

impl Settings {
    pub fn validate(&self) -> AppResult<()> {
        if !(10..=5000).contains(&self.freeze_interval_ms) {
            return Err(AppError::InvalidValue("l'interval de congelació ha d'estar entre 10 i 5000 ms".into()));
        }
        for (trainer, keys) in &self.hotkey_overrides {
            for (key, accel) in keys {
                if !accel.is_empty() && !crate::trainers::model::valid_hotkey(accel) {
                    return Err(AppError::InvalidValue(format!("drecera «{accel}» no vàlida ({trainer} / {key})")));
                }
            }
        }
        Ok(())
    }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let settings = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) if s.validate().is_ok() => s,
                _ => {
                    // No el sobreescrivim: el guardem a part perquè l'usuari no perdi res
                    let backup = path.with_extension("json.bak");
                    let _ = std::fs::rename(&path, &backup);
                    tracing::warn!(backup = %backup.display(), "settings.json invàlid; es fan servir els valors per defecte");
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        Self { path, inner: RwLock::new(settings) }
    }

    pub fn get(&self) -> Settings {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn save(&self, settings: Settings) -> AppResult<()> {
        settings.validate()?;
        let text = serde_json::to_string_pretty(&settings).map_err(|e| AppError::Os(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::Os(e.to_string()))?;
        }
        // Escriptura atòmica: primer un fitxer temporal i després el renom
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &self.path)).map_err(|e| AppError::Os(e.to_string()))?;
        *self.inner.write().unwrap_or_else(|e| e.into_inner()) = settings;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_loads_and_keeps_broken_files_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get().freeze_interval_ms, 100);

        let mut s = store.get();
        s.freeze_interval_ms = 50;
        s.hotkey_overrides.entry("joc".into()).or_default().insert("vida".into(), "Ctrl+F1".into());
        store.save(s).unwrap();
        assert_eq!(SettingsStore::load(path.clone()).get().hotkey_overrides["joc"]["vida"], "Ctrl+F1");

        let mut bad = store.get();
        bad.freeze_interval_ms = 1;
        assert!(store.save(bad).is_err());

        std::fs::write(&path, "{ trencat").unwrap();
        assert_eq!(SettingsStore::load(path.clone()).get().freeze_interval_ms, 100);
        assert!(path.with_extension("json.bak").exists());
    }
}
