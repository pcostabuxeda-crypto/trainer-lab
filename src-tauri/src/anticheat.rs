//! Detecció d'anti-cheat abans d'adjuntar-se a un joc.
//!
//! Política (decidida amb l'usuari):
//! - **Bloqueig** si el procés objectiu té carregat un mòdul d'anti-cheat: vol dir que el
//!   joc s'està executant amb protecció (i probablement en línia).
//! - **Avís** (cal confirmar) si hi ha un anti-cheat en marxa al sistema però no dins del
//!   joc, p. ex. Vanguard, que s'executa sempre.
//!
//! No hi ha cap mecanisme per saltar-se el bloqueig.

/// Prefixos (en minúscules) de mòduls d'anti-cheat carregats dins d'un joc.
const MODULES: &[(&str, &str)] = &[
    ("easyanticheat", "Easy Anti-Cheat"),
    ("eosanticheat", "Easy Anti-Cheat (EOS)"),
    ("beclient", "BattlEye"),
    ("vgk", "Riot Vanguard"),
    ("xigncode", "XIGNCODE3"),
    ("gameguard", "nProtect GameGuard"),
    ("npggnt", "nProtect GameGuard"),
    ("anticheatsdk", "EA AntiCheat"),
];

/// Processos (en minúscules, sense `.exe`) d'anti-cheat en marxa al sistema.
const PROCESSES: &[(&str, &str)] = &[
    ("easyanticheat", "Easy Anti-Cheat"),
    ("easyanticheat_eos", "Easy Anti-Cheat (EOS)"),
    ("beservice", "BattlEye"),
    ("beservice_x64", "BattlEye"),
    ("vgc", "Riot Vanguard"),
    ("vgtray", "Riot Vanguard"),
    ("faceit", "FACEIT"),
    ("faceitservice", "FACEIT"),
    ("eaanticheat.gameservice", "EA AntiCheat"),
    ("gameguard", "nProtect GameGuard"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AntiCheatReport {
    /// Anti-cheats carregats dins del joc: bloquegen
    pub in_game: Vec<String>,
    /// Anti-cheats en marxa al sistema: només avisen
    pub system: Vec<String>,
}

pub fn check(module_names: &[String], process_names: &[String]) -> AntiCheatReport {
    let matches = |names: &[String], table: &[(&str, &str)], exact: bool| {
        let mut found: Vec<String> = names
            .iter()
            .filter_map(|n| {
                let n = n.to_lowercase();
                let stem = n.strip_suffix(".exe").or_else(|| n.strip_suffix(".dll")).unwrap_or(&n);
                table
                    .iter()
                    .find(|(k, _)| if exact { stem == *k } else { stem.starts_with(k) })
                    .map(|(_, label)| label.to_string())
            })
            .collect();
        found.sort();
        found.dedup();
        found
    };
    AntiCheatReport { in_game: matches(module_names, MODULES, false), system: matches(process_names, PROCESSES, true) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn detects_modules_inside_game_and_system_processes() {
        let r = check(
            &s(&["Game.exe", "KERNEL32.DLL", "EasyAntiCheat_EOS.dll", "BEClient_x64.dll"]),
            &s(&["explorer.exe", "vgc.exe", "vgtray.exe", "BEService.exe"]),
        );
        assert_eq!(r.in_game, s(&["BattlEye", "Easy Anti-Cheat"]));
        assert_eq!(r.system, s(&["BattlEye", "Riot Vanguard"]));
    }

    #[test]
    fn clean_game_has_no_findings_and_no_false_positives() {
        let r = check(&s(&["test-game.exe", "ntdll.dll"]), &s(&["svchost.exe", "vgcfoo.exe", "faceitxyz.exe"]));
        assert_eq!(r, AntiCheatReport::default());
    }
}
