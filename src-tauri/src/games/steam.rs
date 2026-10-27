//! Jocs de Steam.
//!
//! 1. La carpeta de Steam surt del registre (`HKCU\Software\Valve\Steam\SteamPath`).
//! 2. `steamapps/libraryfolders.vdf` llista totes les biblioteques (poden ser a altres discs).
//! 3. Cada joc instal·lat té un `steamapps/appmanifest_<appid>.acf` amb el nom i la carpeta,
//!    que és a `steamapps/common/<installdir>`.

use std::path::{Path, PathBuf};

use super::registry::{self, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use super::{GameEntry, Store, vdf};

/// `StateFlags` de l'appmanifest: bit 4 = completament instal·lat.
const FULLY_INSTALLED: u32 = 4;
/// Paquets de Steam que no són jocs.
const NOT_GAMES: &[&str] = &["228980", "1070560", "1391110", "1628350"];

pub fn root() -> Option<PathBuf> {
    registry::read_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
        .or_else(|| registry::read_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"))
        // El registre la guarda amb barres `/`
        .map(|p| PathBuf::from(p.replace('/', "\\")))
        .filter(|p| p.is_dir())
}

pub fn scan(root: &Path) -> Vec<GameEntry> {
    // Les rutes de libraryfolders.vdf tenen les majúscules correctes (el registre no)
    let mut libraries: Vec<PathBuf> = Vec::new();
    if let Some(v) = std::fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")).ok().and_then(|t| vdf::parse(&t))
        && let Some(lf) = v.get("libraryfolders")
    {
        libraries.extend(lf.entries().filter_map(|(_, lib)| lib.str("path").map(PathBuf::from)));
    }
    if !libraries.iter().any(|l| same_path(l, root)) {
        libraries.push(root.to_path_buf());
    }

    let mut games = Vec::new();
    for lib in &libraries {
        let apps = lib.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Some(v) = std::fs::read_to_string(e.path()).ok().and_then(|t| vdf::parse(&t)) else {
                tracing::debug!(file = %e.path().display(), "appmanifest il·legible");
                continue;
            };
            let Some(app) = v.get("AppState") else { continue };
            let (Some(id), Some(title), Some(dir)) = (app.str("appid"), app.str("name"), app.str("installdir")) else {
                continue;
            };
            let flags: u32 = app.str("StateFlags").and_then(|f| f.parse().ok()).unwrap_or(0);
            let install_dir = apps.join("common").join(dir);
            if flags & FULLY_INSTALLED == 0 || NOT_GAMES.contains(&id) || !install_dir.is_dir() {
                continue;
            }
            games.push(GameEntry::new(Store::Steam, id, title, &install_dir, None));
        }
    }
    games
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(dir: &Path, id: &str, name: &str, installdir: &str, flags: u32) {
        let text = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"{installdir}\"\n}}\n"
        );
        std::fs::write(dir.join(format!("steamapps/appmanifest_{id}.acf")), text).unwrap();
    }

    #[test]
    fn scans_all_libraries() {
        let (root, extra) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        for d in [root.path(), extra.path()] {
            std::fs::create_dir_all(d.join("steamapps/common")).unwrap();
        }
        let lf = format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
            root.path().display().to_string().replace('\\', "\\\\"),
            extra.path().display().to_string().replace('\\', "\\\\")
        );
        std::fs::write(root.path().join("steamapps/libraryfolders.vdf"), lf).unwrap();

        std::fs::create_dir_all(root.path().join("steamapps/common/Portal 2")).unwrap();
        manifest(root.path(), "620", "Portal 2", "Portal 2", 4);
        std::fs::create_dir_all(extra.path().join("steamapps/common/Game B")).unwrap();
        manifest(extra.path(), "100", "Game B", "Game B", 1030);
        manifest(extra.path(), "200", "A mig instal·lar", "Missing", 1026); // sense bit 4
        manifest(root.path(), "228980", "Steamworks Common Redistributables", "Steamworks Shared", 4);

        let mut games = scan(root.path());
        games.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = games.iter().map(|g| (g.id.as_str(), g.name.as_str())).collect();
        assert_eq!(names, [("steam:100", "Game B"), ("steam:620", "Portal 2")]);
        assert!(games[1].install_dir.ends_with(r"steamapps\common\Portal 2"));
    }
}
