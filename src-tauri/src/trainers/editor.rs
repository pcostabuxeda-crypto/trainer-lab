//! Afegir trucs a les definicions des de l'app (escàner → «Desa com a truc»).
//!
//! Sempre s'escriu a la carpeta de l'usuari:
//! - si el joc ja té una definició de l'usuari, s'hi afegeix el truc;
//! - si només té un exemple inclòs, se'n fa una còpia (amb el mateix `id`, que té prioritat);
//! - si no en té cap, se'n crea una de nova a partir de l'executable.

use std::path::{Path, PathBuf};

use super::model::{CheatDef, GameVersion, ProcessMatch, SCHEMA_VERSION, TrainerDef, validate};
use super::{TrainerSource, TrainerStore};
use crate::error::{AppError, AppResult};
use crate::memory::process::Arch;

/// Identificador a partir del nom de l'exe: `Slime Rancher.exe` → `slime-rancher`.
pub fn slug(exe: &str) -> String {
    let stem = exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe);
    let mut out = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "joc".into() } else { out }
}

/// Afegeix `cheat` a la definició de l'executable i la desa. Retorna la definició i la ruta.
pub fn add_cheat(store: &TrainerStore, exe: &str, arch: Arch, sha256: &str, cheat: CheatDef) -> AppResult<(TrainerDef, PathBuf)> {
    let existing = store.catalog.read().unwrap_or_else(|e| e.into_inner()).for_executable(exe).cloned();
    let (mut def, path) = match existing {
        Some(t) if t.source == TrainerSource::User => (t.def, t.path),
        Some(t) => {
            let path = store.user_dir.join(format!("{}.json", t.def.id));
            (t.def, path)
        }
        None => {
            let id = unique_id(store, &slug(exe));
            let def = TrainerDef {
                schema: None,
                schema_version: SCHEMA_VERSION,
                id: id.clone(),
                name: exe.trim_end_matches(".exe").to_string(),
                online: false,
                process: ProcessMatch { executable: exe.to_string(), arch: Some(arch) },
                store_ids: None,
                versions: if sha256.len() == 64 {
                    vec![GameVersion { label: "versió on es va crear".into(), sha256: sha256.into() }]
                } else {
                    Vec::new()
                },
                cheats: Vec::new(),
            };
            (def, store.user_dir.join(format!("{id}.json")))
        }
    };

    if def.cheats.iter().any(|c| c.id == cheat.id) {
        return Err(AppError::InvalidValue(format!("ja hi ha un truc amb l'id «{}»", cheat.id)));
    }
    def.cheats.push(cheat);
    validate(&def).map_err(|errs| AppError::InvalidValue(errs.join(" · ")))?;
    write_json(&path, &def)?;
    tracing::info!(trainer = %def.id, path = %path.display(), "truc afegit a la definició");
    store.reload();
    Ok((def, path))
}

fn unique_id(store: &TrainerStore, base: &str) -> String {
    let cat = store.catalog.read().unwrap_or_else(|e| e.into_inner());
    let taken = |id: &str| cat.trainers.contains_key(id) || store.user_dir.join(format!("{id}.json")).exists();
    (1..).map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") }).find(|id| !taken(id)).unwrap()
}

/// Escriptura atòmica (temporal + renom): si falla a mig camí, el fitxer original queda intacte.
fn write_json(path: &Path, def: &TrainerDef) -> AppResult<()> {
    let text = serde_json::to_string_pretty(def).map_err(|e| AppError::Os(e.to_string()))? + "\n";
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| AppError::Os(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trainers::model::CheatKind;

    fn cheat(id: &str) -> CheatDef {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": "Vida", "kind": "toggle",
            "target": { "address": { "module": "joc.exe", "offset": "0x100", "pointers": ["0x8"] }, "type": "i32" },
            "on": { "freeze": 100 }
        }))
        .unwrap()
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Slime Rancher.exe"), "slime-rancher");
        assert_eq!(slug("GTA5_x64.EXE"), "gta5-x64");
        assert_eq!(slug("!!!.exe"), "joc");
    }

    #[test]
    fn creates_new_definition_then_appends_and_rejects_duplicates() {
        let (bundled, user) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let store = TrainerStore::new(bundled.path().into(), user.path().into());

        let (def, path) = add_cheat(&store, "Joc.exe", Arch::X64, &"ab".repeat(32), cheat("vida")).unwrap();
        assert_eq!(def.id, "joc");
        assert_eq!(def.versions.len(), 1);
        assert!(path.starts_with(user.path()) && path.exists());
        assert!(store.catalog.read().unwrap().for_executable("joc.exe").is_some(), "es recarrega el catàleg");

        let (def, _) = add_cheat(&store, "Joc.exe", Arch::X64, "", cheat("vida-2")).unwrap();
        assert_eq!(def.cheats.len(), 2, "s'afegeix a la mateixa definició");
        assert!(add_cheat(&store, "Joc.exe", Arch::X64, "", cheat("vida")).is_err(), "id repetit");

        let mut bad = cheat("dolent");
        bad.kind = CheatKind::Slider;
        assert!(add_cheat(&store, "Joc.exe", Arch::X64, "", bad).is_err(), "es valida abans de desar");
    }

    #[test]
    fn copies_bundled_example_to_user_folder() {
        let (bundled, user) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let example = serde_json::json!({
            "schemaVersion": 1, "id": "exemple", "name": "Exemple", "process": { "executable": "joc.exe" },
            "cheats": [cheat("original")]
        });
        std::fs::write(bundled.path().join("exemple.json"), example.to_string()).unwrap();
        let store = TrainerStore::new(bundled.path().into(), user.path().into());

        let (def, path) = add_cheat(&store, "joc.exe", Arch::X64, "", cheat("nou")).unwrap();
        assert_eq!(path, user.path().join("exemple.json"));
        assert_eq!(def.cheats.len(), 2);
        let original = std::fs::read_to_string(bundled.path().join("exemple.json")).unwrap();
        assert!(!original.contains("\"nou\""), "l'exemple inclòs no es toca");
        assert_eq!(store.catalog.read().unwrap().trainers["exemple"].source, TrainerSource::User);
    }
}
