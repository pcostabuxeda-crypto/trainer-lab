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
