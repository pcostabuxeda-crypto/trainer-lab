//! Jocs de GOG Galaxy: una subclau per joc a `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games`,
//! amb `gameName`, `path` (carpeta) i `exe` (executable principal).

use std::path::PathBuf;

use super::registry::{self, HKEY_LOCAL_MACHINE};
use super::{GameEntry, Store};

const ROOT: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";

pub fn scan() -> Vec<GameEntry> {
    registry::subkeys(HKEY_LOCAL_MACHINE, ROOT)
        .into_iter()
        .filter_map(|id| {
            let key = format!(r"{ROOT}\{id}");
            let name = registry::read_string(HKEY_LOCAL_MACHINE, &key, "gameName")?;
            let dir = PathBuf::from(registry::read_string(HKEY_LOCAL_MACHINE, &key, "path")?);
            let exe = registry::read_string(HKEY_LOCAL_MACHINE, &key, "exe").map(PathBuf::from);
            dir.is_dir().then(|| GameEntry::new(Store::Gog, &id, &name, &dir, exe.as_deref()))
        })
        .collect()
}
