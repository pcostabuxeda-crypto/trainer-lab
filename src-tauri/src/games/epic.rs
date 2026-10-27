//! Jocs d'Epic Games: un manifest JSON (`*.item`) per joc a
//! `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{GameEntry, Store};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    display_name: String,
    install_location: String,
    app_name: String,
    #[serde(default)]
    launch_executable: String,
    #[serde(default, rename = "bIsIncompleteInstall")]
    incomplete: bool,
    #[serde(default)]
    app_categories: Vec<String>,
}

pub fn manifests_dir() -> Option<PathBuf> {
    let base = std::env::var_os("ProgramData")?;
    Some(Path::new(&base).join(r"Epic\EpicGamesLauncher\Data\Manifests")).filter(|p| p.is_dir())
}

pub fn scan(dir: &Path) -> Vec<GameEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("item")))
        .filter_map(|e| {
            let m: Manifest = std::fs::read_to_string(e.path())
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .or_else(|| {
                    tracing::debug!(file = %e.path().display(), "manifest d'Epic il·legible");
                    None
                })?;
            // Els complements i les apps que no són jocs no tenen la categoria "games"
            let is_game = m.app_categories.is_empty() || m.app_categories.iter().any(|c| c == "games");
            let dir = PathBuf::from(&m.install_location);
            if m.incomplete || !is_game || !dir.is_dir() {
                return None;
            }
            let exe = (!m.launch_executable.is_empty()).then(|| dir.join(m.launch_executable.replace('/', "\\")));
            Some(GameEntry::new(Store::Epic, &m.app_name, &m.display_name, &dir, exe.as_deref()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_manifests_and_skips_incomplete_or_non_games() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("Joc");
        std::fs::create_dir_all(&game).unwrap();
        let item = |name: &str, extra: &str| {
            format!(
                r#"{{ "DisplayName": "{name}", "InstallLocation": "{}", "AppName": "{name}Id", "LaunchExecutable": "Bin/Joc.exe", "AppCategories": ["public", "games"] {extra} }}"#,
                game.display().to_string().replace('\\', "\\\\")
            )
        };
        std::fs::write(dir.path().join("a.item"), item("Joc", "")).unwrap();
        std::fs::write(dir.path().join("b.item"), item("Incomplet", r#", "bIsIncompleteInstall": true"#)).unwrap();
        std::fs::write(dir.path().join("c.item"), item("App", "").replace(r#""games""#, r#""applications""#)).unwrap();
        std::fs::write(dir.path().join("d.item"), "{ trencat").unwrap();

        let games = scan(dir.path());
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].id, "epic:JocId");
        assert!(games[0].exe.as_deref().is_some_and(|e| e.ends_with(r"Joc\Bin\Joc.exe")));
    }
}
