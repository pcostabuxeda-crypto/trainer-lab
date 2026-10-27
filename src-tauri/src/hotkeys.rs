//! Dreceres globals (funcionen encara que el joc tingui el focus).
//!
//! Només es registren mentre hi ha un joc adjuntat amb trainer, i es desregistren en
//! desadjuntar: així Trainer Lab no captura tecles quan no cal.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};
use ts_rs::TS;

use crate::session::SessionState;
use crate::settings::SettingsStore;
use crate::trainers::engine::CheatInput;
use crate::trainers::model::{CheatKind, TrainerDef};

#[derive(Debug, Clone, Copy)]
enum Action {
    Toggle,
    Press,
    Increase,
    Decrease,
}

/// Drecera registrada → (truc, acció)
#[derive(Default)]
pub struct HotkeyMap(Mutex<HashMap<u32, (String, Action)>>);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HotkeyFired {
    pub cheat_id: String,
    pub ok: bool,
    pub message: Option<String>,
}

/// Drecera efectiva per a una clau (`id` o `id:increase`): la de l'usuari o la de la definició.
pub fn effective(overrides: Option<&std::collections::BTreeMap<String, String>>, key: &str, default: Option<&String>) -> Option<String> {
    match overrides.and_then(|o| o.get(key)) {
        Some(o) if o.is_empty() => None,
        Some(o) => Some(o.clone()),
        None => default.cloned(),
    }
}

/// Registra les dreceres del trainer. Retorna els errors per clau (p. ex. una drecera
/// que ja fa servir un altre programa).
pub fn register(app: &AppHandle, def: &TrainerDef) -> Vec<(String, String)> {
    unregister_all(app);
    let settings = app.state::<SettingsStore>().get();
    let overrides = settings.hotkey_overrides.get(&def.id);
    let mut wanted: Vec<(String, String, Action)> = Vec::new();
    for c in &def.cheats {
        match c.kind {
            CheatKind::Toggle | CheatKind::Button => {
                let action = if c.kind == CheatKind::Toggle { Action::Toggle } else { Action::Press };
                if let Some(k) = effective(overrides, &c.id, c.hotkey.as_ref()) {
                    wanted.push((c.id.clone(), k, action));
                }
            }
            CheatKind::Slider => {
                let hk = c.hotkeys.clone().unwrap_or_default();
                for (suffix, default, action) in [("increase", hk.increase, Action::Increase), ("decrease", hk.decrease, Action::Decrease)] {
                    if let Some(k) = effective(overrides, &format!("{}:{suffix}", c.id), default.as_ref()) {
                        wanted.push((c.id.clone(), k, action));
                    }
                }
            }
            CheatKind::Value => {}
        }
    }

    let mut errors = Vec::new();
    let map = app.state::<HotkeyMap>();
    let mut map = map.0.lock().unwrap_or_else(|e| e.into_inner());
    for (cheat, accel, action) in wanted {
        let key = match action {
            Action::Increase => format!("{cheat}:increase"),
            Action::Decrease => format!("{cheat}:decrease"),
            _ => cheat.clone(),
        };
        let result = Shortcut::from_str(&accel)
            .map_err(|e| e.to_string())
            .and_then(|s| app.global_shortcut().register(s).map(|_| s).map_err(|e| e.to_string()));
        match result {
            Ok(s) => {
                map.insert(s.id(), (cheat, action));
            }
            Err(e) => {
                tracing::warn!(key, accel, "no s'ha pogut registrar la drecera: {e}");
                errors.push((key, format!("«{accel}» no disponible: {e}")));
            }
        }
    }
    tracing::info!(count = map.len(), "dreceres registrades");
    errors
}

pub fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    app.state::<HotkeyMap>().0.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Es crida quan es prem qualsevol drecera registrada.
pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let Some((cheat, action)) = app.state::<HotkeyMap>().0.lock().unwrap_or_else(|e| e.into_inner()).get(&shortcut.id()).cloned()
    else {
        return;
    };
    let result = app.state::<SessionState>().with_trainer(|s, t| {
        let input = match action {
            Action::Toggle => CheatInput::Toggle { on: !t.is_active(&cheat) },
            Action::Press => CheatInput::Press,
            Action::Increase => CheatInput::Step { up: true },
            Action::Decrease => CheatInput::Step { up: false },
        };
        t.apply(&cheat, input, s.process.as_ref(), &s.modules, &s.freezer)
    });
    let _ = app.emit(
        "hotkey-fired",
        HotkeyFired { cheat_id: cheat, ok: result.is_ok(), message: result.err().map(|e| e.to_string()) },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_win_and_empty_disables() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("a".to_string(), "Ctrl+F1".to_string());
        o.insert("b".to_string(), String::new());
        let f1 = "F1".to_string();
        assert_eq!(effective(Some(&o), "a", Some(&f1)), Some("Ctrl+F1".into()));
        assert_eq!(effective(Some(&o), "b", Some(&f1)), None);
        assert_eq!(effective(Some(&o), "c", Some(&f1)), Some("F1".into()));
        assert_eq!(effective(None, "c", None), None);
    }
}
