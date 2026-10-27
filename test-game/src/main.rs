//! Joc de prova de Trainer Lab.
//!
//! Simula les dades típiques d'un joc (vida, munició, diners, velocitat, nom) perquè
//! puguis provar l'escàner i els trucs sense tocar cap joc real.
//!
//! Com està muntat, a propòsit:
//! - `Player` viu al *heap* (`Box`): la seva adreça canvia a cada execució, com en un joc real.
//! - `ANCHOR` és una variable estàtica (secció `.data` de l'executable): comença amb 8 bytes
//!   fixos (`MAGIC`) i després guarda un punter al `Player`. Un trainer pot:
//!     1. trobar `MAGIC` amb un escaneig AOB (patró de bytes),
//!     2. llegir el punter que hi ha just després (+8),
//!     3. sumar l'offset del camp que vulgui (pointer chain).
//!   Així el truc funciona encara que recompilis el joc i canviïn les adreces.

use std::ffi::c_void;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicPtr, Ordering};

/// Dades del jugador. `repr(C)` fixa l'ordre i els offsets dels camps:
/// health 0x00 · ammo 0x04 · money 0x08 · speed 0x10 · name 0x14
#[repr(C)]
struct Player {
    health: i32,
    ammo: i32,
    money: i64,
    speed: f32,
    name: [u8; 16],
}

#[repr(C)]
struct Anchor {
    magic: [u8; 8],
    player: AtomicPtr<Player>,
}

/// Bytes ASCII "TLABANCH" = 54 4C 41 42 41 4E 43 48. No els escrivim enlloc més del
/// programa perquè el patró AOB només aparegui un cop.
static ANCHOR: Anchor = Anchor {
    magic: [0x54, 0x4C, 0x41, 0x42, 0x41, 0x4E, 0x43, 0x48],
    player: AtomicPtr::new(std::ptr::null_mut()),
};

unsafe extern "system" {
    // kernel32 ja està enllaçat per std; només volem l'adreça base del nostre .exe
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

fn main() {
    let debug = std::env::args().any(|a| a == "--debug");
    let mut name = [0u8; 16];
    name[..5].copy_from_slice(b"Heroi");
    // `Box::into_raw`: el jugador viu fins al final del programa i només hi accedim per punter
    let player = Box::into_raw(Box::new(Player { health: 100, ammo: 30, money: 500, speed: 1.0, name }));
    ANCHOR.player.store(player, Ordering::SeqCst);

    println!("Trainer Lab · joc de prova (PID {})", std::process::id());
    if debug {
        let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
        let anchor = &ANCHOR as *const Anchor as usize;
        println!("  base del mòdul : {base:#x}");
        println!("  ANCHOR         : {anchor:#x} (test-game.exe+{:#x})", anchor - base);
        println!("  Player (heap)  : {:#x}", player as usize);
    }
    println!("Ordres: [d] rebre mal  [s] disparar  [b] comprar  [r] córrer  [Enter] refrescar  [q] sortir\n");

    let mut distance = 0.0f32;
    let stdin = io::stdin();
    loop {
        print_status(player, distance);
        print!("> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        // Llegim sempre a través de l'àncora (i amb volatile) perquè el compilador no
        // guardi els valors en registres: així veiem el que el trainer hi ha escrit.
        let p = ANCHOR.player.load(Ordering::SeqCst);
        match line.trim() {
            "d" => update(p, |pl| pl.health -= 15),
            "s" => update(p, |pl| {
                if pl.ammo > 0 {
                    pl.ammo -= 1
                } else {
                    println!("Sense munició!")
                }
            }),
            "b" => update(p, |pl| {
                if pl.money >= 120 {
                    pl.money -= 120
                } else {
                    println!("No tens prou diners!")
                }
            }),
            "r" => distance += read(p).speed * 10.0,
            "q" => break,
            _ => {}
        }
        if read(p).health <= 0 {
            println!("Has mort! Tornes amb 100 de vida.");
            update(p, |pl| pl.health = 100);
        }
    }
}

fn read(p: *mut Player) -> Player {
    unsafe { std::ptr::read_volatile(p) }
}

fn update(p: *mut Player, f: impl FnOnce(&mut Player)) {
    let mut pl = read(p);
    f(&mut pl);
    unsafe { std::ptr::write_volatile(p, pl) };
}

fn print_status(p: *mut Player, distance: f32) {
    let pl = read(p);
    let name = String::from_utf8_lossy(&pl.name);
    println!(
        "[{}] vida {:>4} · munició {:>3} · diners {:>6} · velocitat {:.1} · distància {:.0}",
        name.trim_end_matches('\0'),
        pl.health,
        pl.ammo,
        pl.money,
        pl.speed,
        distance
    );
}
