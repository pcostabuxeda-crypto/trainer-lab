//! Parser mínim del format KeyValues (VDF) de Valve, el que fan servir
//! `libraryfolders.vdf` i `appmanifest_*.acf`:
//! ```text
//! "AppState"
//! {
//!     "appid"      "620"
//!     "installdir" "Portal 2"
//! }
//! ```
//! Només text: claus i valors entre cometes, i blocs amb `{ }`. Prou per a aquests fitxers.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Vdf {
    Str(String),
    Obj(BTreeMap<String, Vdf>),
}

impl Vdf {
    /// Accés per clau, sense distingir majúscules (Steam no és consistent).
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Obj(m) => m.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Str(s) => Some(s),
            Vdf::Obj(_) => None,
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = (&String, &Vdf)> {
        let m = match self {
            Vdf::Obj(m) => Some(m),
            Vdf::Str(_) => None,
        };
        m.into_iter().flatten()
    }
}

/// Retorna l'objecte arrel (que conté la clau de primer nivell, p. ex. `AppState`).
pub fn parse(text: &str) -> Option<Vdf> {
    let mut tokens = tokenize(text)?.into_iter().peekable();
    let root = parse_obj(&mut tokens, true)?;
    Some(Vdf::Obj(root))
}

#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Tok::Open),
            '}' => out.push(Tok::Close),
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next()? {
                        '"' => break,
                        // Steam escriu les rutes amb `\\`
                        '\\' => match chars.next()? {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            other => s.push(other),
                        },
                        ch => s.push(ch),
                    }
                }
                out.push(Tok::Str(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            _ => return None,
        }
    }
    Some(out)
}

fn parse_obj(tokens: &mut std::iter::Peekable<std::vec::IntoIter<Tok>>, root: bool) -> Option<BTreeMap<String, Vdf>> {
    let mut map = BTreeMap::new();
    loop {
        match tokens.next() {
            None if root => return Some(map),
            Some(Tok::Close) if !root => return Some(map),
            Some(Tok::Str(key)) => {
                let value = match tokens.next()? {
                    Tok::Str(v) => Vdf::Str(v),
                    Tok::Open => Vdf::Obj(parse_obj(tokens, false)?),
                    Tok::Close => return None,
                };
                map.insert(key, value);
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folders() {
        let text = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps" { "228980" "123" "620" "456" }
	}
	// comentari
	"1" { "path" "D:\\SteamLibrary" }
}"#;
        let v = parse(text).unwrap();
        let lf = v.get("LibraryFolders").unwrap();
        assert_eq!(lf.get("0").unwrap().str("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(lf.get("1").unwrap().str("path"), Some(r"D:\SteamLibrary"));
        assert_eq!(lf.get("0").unwrap().get("apps").unwrap().entries().count(), 2);
    }

    #[test]
    fn rejects_broken_files() {
        assert!(parse(r#""a" { "b" "c" "#).is_none(), "falta tancar");
        assert!(parse(r#""a" "b" }"#).is_none(), "tancament de més");
        assert!(parse(r#""a "#).is_none(), "cometes sense tancar");
        assert!(parse("a b").is_none(), "sense cometes");
    }
}
