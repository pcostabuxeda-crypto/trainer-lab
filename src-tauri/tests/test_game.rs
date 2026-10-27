//! Test d'integració: obre el joc de prova real, troba la vida amb AOB + pointer chain,
//! l'escriu, la congela i comprova que el joc veu els valors nous.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use trainer_lab_lib::memory::freeze::Freezer;
use trainer_lab_lib::memory::locator::{self, Locator};
use trainer_lab_lib::memory::process::{Arch, Process};
use trainer_lab_lib::memory::{Memory, WriteMode};
use trainer_lab_lib::trainers::engine::{ActiveTrainer, CheatInput, VersionInfo};
use trainer_lab_lib::trainers::model::{TrainerDef, validate};

struct Game(Child);

impl Drop for Game {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn start_game() -> (Game, BufReader<std::process::ChildStdout>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let status = Command::new(env!("CARGO")).args(["build", "-q", "-p", "test-game"]).current_dir(&root).status().unwrap();
    assert!(status.success(), "no s'ha pogut compilar test-game");
    let mut child = Command::new(root.join("target/debug/test-game.exe"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    // Esperem la línia d'estat inicial: llavors el jugador ja és a memòria
    let mut line = String::new();
    while !line.contains("vida") {
        line.clear();
        out.read_line(&mut line).unwrap();
    }
    (Game(child), out)
}

fn status_line(game: &mut Game, out: &mut BufReader<std::process::ChildStdout>, cmd: &str) -> String {
    writeln!(game.0.stdin.as_mut().unwrap(), "{cmd}").unwrap();
    let mut line = String::new();
    while !line.contains("vida") {
        line.clear();
        out.read_line(&mut line).unwrap();
    }
    line
}

fn health_locator(field_offset: &str) -> Locator {
    serde_json::from_str(&format!(
        r#"{{ "aob": {{ "pattern": "54 4C 41 42 41 4E 43 48", "offset": 8 }}, "pointers": ["{field_offset}"] }}"#
    ))
    .unwrap()
}

#[test]
fn reads_writes_and_freezes_the_test_game() {
    let (mut game, mut out) = start_game();
    let p = Process::open(game.0.id()).unwrap();
    assert_eq!(p.arch, Arch::X64);
    assert_eq!(p.name, "test-game.exe");
    let modules = p.modules().unwrap();

    // vida: offset 0x0 dins de Player · diners: 0x8
    let health = locator::resolve(&p, &modules, &health_locator("0x0")).unwrap();
    let money = locator::resolve(&p, &modules, &health_locator("0x8")).unwrap();
    assert_eq!(money - health, 8);

    let mut b = [0u8; 4];
    p.read_bytes(health, &mut b).unwrap();
    assert_eq!(i32::from_le_bytes(b), 100, "vida inicial");

    p.write_bytes(health, &777i32.to_le_bytes(), WriteMode::Data).unwrap();
    p.write_bytes(money, &123456i64.to_le_bytes(), WriteMode::Data).unwrap();
    let line = status_line(&mut game, &mut out, "");
    assert!(line.contains("vida  777") && line.contains("123456"), "el joc veu els valors nous: {line}");

    // Congelem la vida a 100 i rebem mal: la vida torna a 100
    let process = Arc::new(p);
    let freezer = Freezer::start(process.clone(), 10);
    freezer.set("vida", health, 100i32.to_le_bytes().to_vec());
    std::thread::sleep(Duration::from_millis(50));
    status_line(&mut game, &mut out, "d"); // -15
    std::thread::sleep(Duration::from_millis(50));
    let line = status_line(&mut game, &mut out, "");
    assert!(line.contains("vida  100"), "la congelació manté la vida: {line}");

    // En tancar el joc, el procés ho detecta
    drop(freezer);
    let _ = status_line(&mut game, &mut out, "");
    writeln!(game.0.stdin.as_mut().unwrap(), "q").unwrap();
    assert!(process.wait_exit(5000), "detecta que el joc s'ha tancat");
    assert!(process.read_bytes(health, &mut b).is_err());
}

#[test]
fn example_definition_works_on_the_test_game() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../trainers/test-game.json");
    let def: TrainerDef = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    validate(&def).unwrap();

    let (mut game, mut out) = start_game();
    let p = Arc::new(Process::open(game.0.id()).unwrap());
    let modules = p.modules().unwrap();
    let version = VersionInfo::check(&def, String::new());
    let mut t = ActiveTrainer::new(def, version, p.as_ref(), &modules);
    let freezer = Freezer::start(p.clone(), 10);
    let mut apply = |id: &str, input| t.apply(id, input, p.as_ref(), &modules, &freezer).unwrap();

    apply("vida-infinita", CheatInput::Toggle { on: true });
    apply("afegir-municio", CheatInput::Press);
    apply("diners", CheatInput::Set { value: 4242.0 });
    apply("velocitat", CheatInput::Set { value: 3.0 });
    std::thread::sleep(Duration::from_millis(30));
    status_line(&mut game, &mut out, "d"); // rep mal, però la vida està congelada
    std::thread::sleep(Duration::from_millis(30));
    let line = status_line(&mut game, &mut out, "r");
    assert!(line.contains("vida  100"), "{line}");
    assert!(line.contains("munició  80"), "30 + 50: {line}");
    assert!(line.contains("diners   4242"), "{line}");
    assert!(line.contains("velocitat 3.0") && line.contains("distància 30"), "{line}");

    // Desactivar la vida restaura el valor que hi havia en activar-la (100)
    apply("vida-infinita", CheatInput::Toggle { on: false });
    let line = status_line(&mut game, &mut out, "d");
    assert!(line.contains("vida   85"), "{line}");
    writeln!(game.0.stdin.as_mut().unwrap(), "q").unwrap();
}

#[test]
fn scanner_finds_health_and_its_static_pointer() {
    use trainer_lab_lib::memory::scanner::{ScanFilter, find_static_pointers, first_scan};
    use trainer_lab_lib::memory::value::ValueType;

    let (mut game, mut out) = start_game();
    let p = Process::open(game.0.id()).unwrap();
    let modules = p.modules().unwrap();
    let health = locator::resolve(&p, &modules, &health_locator("0x0")).unwrap();

    // 1. Vida = 100: surten moltes adreces amb un 100
    let mut scan = first_scan(&p, ValueType::I32, 100.0);
    assert!(scan.count() > 1, "hi ha més d'un 100 a la memòria");
    // 2. Rep mal (85) i refina: «exacte 85» i després «no ha canviat»
    status_line(&mut game, &mut out, "d");
    scan.next_scan(&p, ScanFilter::Exact { value: 85.0 });
    scan.next_scan(&p, ScanFilter::Unchanged);
    let hits = scan.page(&p, &modules, 0, 50);
    assert!(hits.iter().any(|h| h.address == format!("{health:#x}")), "la vida és entre els resultats: {hits:?}");
    let h = hits.iter().find(|h| h.address == format!("{health:#x}")).unwrap();
    assert_eq!(h.module_offset, None, "la vida és al heap (dinàmica)");

    // 3. El punter estàtic és l'ANCHOR del joc: test-game.exe+offset, amb offset 0 dins del Player
    let candidates = find_static_pointers(&p, &modules, health, 0x100, 10);
    assert!(
        candidates.iter().any(|c| c.module == "test-game.exe" && c.pointer_offset == "0x0"),
        "{candidates:?}"
    );
    writeln!(game.0.stdin.as_mut().unwrap(), "q").unwrap();
}
