//! Format de les definicions de trainers (`trainers/<id>.json`).
//!
//! Els structs fan dues feines: serde llegeix el JSON (rebutjant camps desconeguts) i
//! `schemars` en genera el JSON Schema per a l'autocompletat de l'editor. Les regles que un
//! esquema no pot expressar bé (coherència entre camps) són a [`validate`].
//! Documentació del format: docs/PLA_ARQUITECTURA.md §4 i README.

use std::collections::HashSet;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::Shortcut;
use ts_rs::TS;

use crate::memory::aob::Pattern;
use crate::memory::locator::Locator;
use crate::memory::process::Arch;
use crate::memory::value::{ValueType, parse_hex_bytes};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct TrainerDef {
    /// Ruta a l'esquema, per a l'autocompletat de l'editor
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub schema: Option<String>,
    pub schema_version: u32,
    /// Identificador únic: minúscules, xifres i guions
    pub id: String,
    pub name: String,
    /// Si és `true`, Trainer Lab es nega a adjuntar-se (joc en línia)
    #[serde(default)]
    pub online: bool,
    pub process: ProcessMatch,
    /// Identificadors del joc a les botigues, per reconèixer-lo a la Biblioteca
    #[serde(rename = "match", default, skip_serializing_if = "Option::is_none")]
    #[ts(rename = "match", optional)]
    pub store_ids: Option<StoreIds>,
    /// Versions provades. Si no n'hi ha cap, la versió surt com a «sense informació»
    #[serde(default)]
    pub versions: Vec<GameVersion>,
    pub cheats: Vec<CheatDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProcessMatch {
    /// Nom de l'executable, p. ex. `game.exe` (sense distingir majúscules)
    pub executable: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub arch: Option<Arch>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct StoreIds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub steam_app_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub epic_app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GameVersion {
    pub label: String,
    /// SHA-256 de l'executable, en hex
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CheatKind {
    /// Activar/desactivar: congela un valor, l'escriu un cop o aplica un patch
    Toggle,
    /// Valor triat dins d'un rang
    Slider,
    /// Camp numèric + «Aplica»
    Value,
    /// Acció puntual (sumar o fixar)
    Button,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct Target {
    pub address: Locator,
    #[serde(rename = "type")]
    pub value_type: ValueType,
}

/// Què fa un toggle en activar-se. Exactament un de `freeze`, `write` o `patch`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleOn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub freeze: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub write: Option<f64>,
    /// Bytes hex que substitueixen el codi, p. ex. `"90 90 90"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub patch: Option<String>,
    /// Bytes que hi ha d'haver abans d'aplicar el patch. Si no coincideixen, no s'escriu res
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expect: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToggleOff {
    /// Torna a posar el valor (o els bytes) que hi havia en activar-lo
    #[default]
    Restore,
    /// Només deixa de congelar; no toca la memòria
    None,
}

/// Acció d'un botó. Exactament un de `add` o `set`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ButtonAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub add: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub set: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SliderHotkeys {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub increase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub decrease: Option<String>,
}

/// Un truc. Els camps vàlids depenen de `kind` (vegeu [`validate`]).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CheatDef {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    pub kind: CheatKind,
    pub target: Target,
    /// toggle
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<ToggleOn>,
    /// toggle
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub off: Option<ToggleOff>,
    /// slider (obligatori), value (opcional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max: Option<f64>,
    /// slider
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub step: Option<f64>,
    /// slider, value: valor inicial a la UI
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<f64>,
    /// slider: congelar el valor triat
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub freeze: Option<bool>,
    /// button
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub action: Option<ButtonAction>,
    /// Drecera global per defecte (toggle, button), p. ex. `"F1"` o `"Ctrl+Shift+H"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hotkey: Option<String>,
    /// slider: dreceres per pujar i baixar un `step`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hotkeys: Option<SliderHotkeys>,
}

fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn valid_hotkey(s: &str) -> bool {
    Shortcut::from_str(s).is_ok()
}

/// Comprovacions de coherència. Retorna tots els errors trobats, cadascun amb la ruta del
/// camp (`cheats[2].on.patch: …`) perquè sigui fàcil trobar-lo al fitxer.
pub fn validate(def: &TrainerDef) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    let mut err = |path: String, msg: &str| errs.push(format!("{path}: {msg}"));

    if def.schema_version != SCHEMA_VERSION {
        err("schemaVersion".into(), &format!("versió {} no suportada (s'espera {SCHEMA_VERSION})", def.schema_version));
    }
    if !valid_id(&def.id) {
        err("id".into(), "només minúscules, xifres i guions");
    }
    if !def.process.executable.to_lowercase().ends_with(".exe") {
        err("process.executable".into(), "ha de ser un .exe");
    }
    for (i, v) in def.versions.iter().enumerate() {
        if v.sha256.len() != 64 || !v.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            err(format!("versions[{i}].sha256"), "ha de tenir 64 xifres hex");
        }
    }
    if def.cheats.is_empty() {
        err("cheats".into(), "cal com a mínim un truc");
    }

    let mut ids = HashSet::new();
    let mut hotkeys = HashSet::new();
    for (i, c) in def.cheats.iter().enumerate() {
        let p = |field: &str| format!("cheats[{i}].{field}");
        if !valid_id(&c.id) {
            err(p("id"), "només minúscules, xifres i guions");
        }
        if !ids.insert(c.id.as_str()) {
            err(p("id"), &format!("«{}» està repetit", c.id));
        }

        // Adreça
        let a = &c.target.address;
        if a.offset.is_some() == a.aob.is_some() {
            err(p("target.address"), "cal exactament un de `offset` o `aob`");
        }
        if let Some(aob) = &a.aob
            && let Err(e) = aob.pattern.parse::<Pattern>()
        {
            err(p("target.address.aob.pattern"), &e.to_string());
        }

        let ty = c.target.value_type;
        let check_value = |v: f64| ty.encode(v).map(|_| ()).map_err(|e| e.to_string());
        let not_for_kind = |present: bool, field: &str| present.then(|| (p(field), format!("no s'aplica a kind {:?}", c.kind)));
        let mut extra: Vec<(String, String)> = Vec::new();

        match c.kind {
            CheatKind::Toggle => {
                match &c.on {
                    None => err(p("on"), "un toggle necessita `on`"),
                    Some(on) => {
                        let n = [on.freeze.is_some(), on.write.is_some(), on.patch.is_some()].iter().filter(|x| **x).count();
                        if n != 1 {
                            err(p("on"), "cal exactament un de `freeze`, `write` o `patch`");
                        }
                        for v in on.freeze.iter().chain(&on.write) {
                            if let Err(e) = check_value(*v) {
                                err(p("on"), &e);
                            }
                        }
                        if let Some(patch) = &on.patch {
                            if ty != ValueType::Bytes {
                                err(p("target.type"), "un `patch` necessita el tipus `bytes`");
                            }
                            match (parse_hex_bytes(patch), on.expect.as_deref().map(parse_hex_bytes)) {
                                (Err(e), _) => err(p("on.patch"), &e.to_string()),
                                (_, None) => err(p("on.expect"), "un `patch` necessita `expect` (els bytes originals)"),
                                (_, Some(Err(e))) => err(p("on.expect"), &e.to_string()),
                                (Ok(a), Some(Ok(b))) if a.len() != b.len() => {
                                    err(p("on.expect"), "ha de tenir la mateixa longitud que `patch`")
                                }
                                _ => {}
                            }
                        } else if on.expect.is_some() {
                            err(p("on.expect"), "només s'aplica amb `patch`");
                        } else if ty == ValueType::Bytes {
                            err(p("target.type"), "el tipus `bytes` només es pot fer servir amb `patch`");
                        }
                    }
                }
                extra.extend(not_for_kind(c.min.is_some() || c.max.is_some() || c.step.is_some(), "min/max/step"));
                extra.extend(not_for_kind(c.action.is_some(), "action"));
                extra.extend(not_for_kind(c.hotkeys.is_some(), "hotkeys"));
            }
            CheatKind::Slider => {
                if !ty.is_numeric() {
                    err(p("target.type"), "un slider necessita un tipus numèric");
                }
                match (c.min, c.max) {
                    (Some(min), Some(max)) if min < max => {
                        for v in [Some(min), Some(max), c.default].into_iter().flatten() {
                            if let Err(e) = check_value(v) {
                                err(p("min/max/default"), &e);
                            }
                        }
                        if c.default.is_some_and(|d| d < min || d > max) {
                            err(p("default"), "ha d'estar entre `min` i `max`");
                        }
                    }
                    _ => err(p("min/max"), "un slider necessita `min` < `max`"),
                }
                if c.step.is_some_and(|s| s <= 0.0) {
                    err(p("step"), "ha de ser positiu");
                }
                extra.extend(not_for_kind(c.on.is_some() || c.off.is_some(), "on/off"));
                extra.extend(not_for_kind(c.action.is_some(), "action"));
                extra.extend(not_for_kind(c.hotkey.is_some(), "hotkey (fes servir `hotkeys`)"));
            }
            CheatKind::Value => {
                if ty == ValueType::Bytes {
                    err(p("target.type"), "el tipus `bytes` no és vàlid aquí");
                }
                if let (Some(min), Some(max)) = (c.min, c.max)
                    && min > max
                {
                    err(p("min/max"), "`min` ha de ser <= `max`");
                }
                if let Some(d) = c.default
                    && let Err(e) = check_value(d)
                {
                    err(p("default"), &e);
                }
                extra.extend(not_for_kind(c.on.is_some() || c.off.is_some(), "on/off"));
                extra.extend(not_for_kind(c.action.is_some() || c.step.is_some(), "action/step"));
                extra.extend(not_for_kind(c.hotkey.is_some() || c.hotkeys.is_some(), "hotkey"));
            }
            CheatKind::Button => {
                match &c.action {
                    None => err(p("action"), "un botó necessita `action`"),
                    Some(a) if a.add.is_some() == a.set.is_some() => err(p("action"), "cal exactament un de `add` o `set`"),
                    Some(a) => {
                        if a.add.is_some() && !ty.is_numeric() {
                            err(p("action.add"), "només per a tipus numèrics");
                        }
                        if let Some(v) = a.set
                            && let Err(e) = check_value(v)
                        {
                            err(p("action.set"), &e);
                        }
                    }
                }
                extra.extend(not_for_kind(c.on.is_some() || c.off.is_some(), "on/off"));
                extra.extend(not_for_kind(c.min.is_some() || c.max.is_some() || c.step.is_some(), "min/max/step"));
                extra.extend(not_for_kind(c.hotkeys.is_some(), "hotkeys"));
            }
        }
        for (path, msg) in extra {
            err(path, &msg);
        }

        let keys = c.hotkey.iter().map(|h| ("hotkey", h)).chain(
            c.hotkeys.iter().flat_map(|h| h.increase.iter().map(|k| ("hotkeys.increase", k)).chain(h.decrease.iter().map(|k| ("hotkeys.decrease", k)))),
        );
        for (field, key) in keys {
            if !valid_hotkey(key) {
                err(p(field), &format!("drecera «{key}» no vàlida (ex.: \"F1\", \"Ctrl+Shift+H\")"));
            } else if !hotkeys.insert(key.to_lowercase()) {
                err(p(field), &format!("la drecera «{key}» ja la fa servir un altre truc"));
            }
        }
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDR: &str = r#""target": { "address": { "offset": "0x10" }, "type": "i32" }"#;

    fn def(cheats: &str) -> TrainerDef {
        let json = format!(
            r#"{{ "schemaVersion": 1, "id": "joc", "name": "Joc", "process": {{ "executable": "joc.exe" }}, "cheats": [{cheats}] }}"#
        );
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}\n{json}"))
    }

    fn errors(cheats: &str) -> Vec<String> {
        validate(&def(cheats)).err().unwrap_or_default()
    }

    #[test]
    fn valid_cheats_of_every_kind() {
        let ok = format!(
            r#"{{ "id": "a", "name": "A", "kind": "toggle", {ADDR}, "on": {{ "freeze": 100 }}, "hotkey": "F1" }},
               {{ "id": "b", "name": "B", "kind": "slider", {ADDR}, "min": 0, "max": 10, "step": 1, "hotkeys": {{ "increase": "F3", "decrease": "F2" }} }},
               {{ "id": "c", "name": "C", "kind": "value", {ADDR}, "default": 5 }},
               {{ "id": "d", "name": "D", "kind": "button", {ADDR}, "action": {{ "add": 50 }}, "hotkey": "Ctrl+Shift+D" }},
               {{ "id": "e", "name": "E", "kind": "toggle", "target": {{ "address": {{ "aob": {{ "pattern": "FF 4B 10 48" }} }}, "type": "bytes" }},
                  "on": {{ "patch": "90 90 90", "expect": "FF 4B 10" }}, "off": "restore" }}"#
        );
        assert_eq!(errors(&ok), Vec::<String>::new());
    }

    #[test]
    fn reports_each_problem_with_its_path() {
        let e = errors(&format!(
            r#"{{ "id": "A!", "name": "x", "kind": "toggle", {ADDR}, "on": {{ "freeze": 1, "write": 2 }} }},
               {{ "id": "s", "name": "x", "kind": "slider", {ADDR}, "min": 5, "max": 1 }},
               {{ "id": "s", "name": "x", "kind": "button", {ADDR}, "action": {{ "add": 1, "set": 2 }}, "hotkey": "F1" }},
               {{ "id": "p", "name": "x", "kind": "toggle", {ADDR}, "on": {{ "patch": "90 90", "expect": "FF" }}, "hotkey": "f1" }},
               {{ "id": "r", "name": "x", "kind": "value", {ADDR}, "default": 1e12 }}"#
        ));
        let has = |s: &str| e.iter().any(|x| x.contains(s));
        assert!(has("cheats[0].id"), "{e:?}");
        assert!(has("cheats[0].on: cal exactament un"), "{e:?}");
        assert!(has("cheats[1].min/max"), "{e:?}");
        assert!(has("cheats[2].id: «s» està repetit"), "{e:?}");
        assert!(has("cheats[2].action"), "{e:?}");
        assert!(has("cheats[3].target.type"), "patch sense bytes: {e:?}");
        assert!(has("cheats[3].on.expect"), "longitud diferent: {e:?}");
        assert!(has("cheats[3].hotkey: la drecera «f1» ja"), "{e:?}");
        assert!(has("cheats[4].default"), "fora de rang d'i32: {e:?}");
    }

    #[test]
    fn rejects_fields_from_other_kinds_and_unknown_fields() {
        let e = errors(&format!(r#"{{ "id": "a", "name": "A", "kind": "toggle", {ADDR}, "on": {{ "freeze": 1 }}, "min": 0 }}"#));
        assert!(e.iter().any(|x| x.contains("no s'aplica a kind Toggle")), "{e:?}");
        let unknown = r#"{ "schemaVersion": 1, "id": "j", "name": "J", "process": { "executable": "j.exe" }, "cheats": [], "extra": 1 }"#;
        assert!(serde_json::from_str::<TrainerDef>(unknown).is_err());
    }

    /// Genera `schemas/trainer.schema.json` (com ts-rs amb els tipus TS: `npm run types`).
    #[test]
    fn export_json_schema() {
        let schema = schemars::schema_for!(TrainerDef);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../schemas");
        std::fs::create_dir_all(&dir).unwrap();
        let text = serde_json::to_string_pretty(&schema).unwrap() + "
";
        std::fs::write(dir.join("trainer.schema.json"), text).unwrap();
    }

    /// Les definicions d'exemple del repositori han de ser vàlides.
    #[test]
    fn bundled_examples_are_valid() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../trainers");
        let mut n = 0;
        for f in std::fs::read_dir(dir).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|x| x == "json")) {
            let text = std::fs::read_to_string(f.path()).unwrap();
            let def: TrainerDef = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", f.path().display()));
            validate(&def).unwrap_or_else(|e| panic!("{}: {e:?}", f.path().display()));
            n += 1;
        }
        assert!(n > 0);
    }

    #[test]
    fn checks_top_level_fields() {
        let mut d = def(&format!(r#"{{ "id": "a", "name": "A", "kind": "value", {ADDR} }}"#));
        d.schema_version = 2;
        d.process.executable = "joc".into();
        d.versions.push(GameVersion { label: "1.0".into(), sha256: "abc".into() });
        let e = validate(&d).unwrap_err();
        assert_eq!(e.len(), 3, "{e:?}");
    }
}
