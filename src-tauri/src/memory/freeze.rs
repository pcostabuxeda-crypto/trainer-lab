//! Congelació de valors.
//!
//! "Congelar" = reescriure el mateix valor una vegada i una altra. El joc el continua
//! canviant (et treu vida), però al cap de pocs mil·lisegons el tornem a posar al seu lloc.
//! Un fil per sessió recorre els valors congelats cada `interval` ms.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{Memory, WriteMode, pointer};

/// Després de tants errors seguits deixem de congelar aquell valor (l'adreça ja no és vàlida).
const MAX_FAILURES: u32 = 20;

struct Frozen {
    address: u64,
    /// Si n'hi ha, l'adreça es recalcula a cada cicle: (punt de partida, offsets)
    chain: Option<(u64, Vec<i64>)>,
    bytes: Vec<u8>,
    failures: u32,
}

type Entries = Arc<Mutex<BTreeMap<String, Frozen>>>;

pub struct Freezer {
    entries: Entries,
    interval_ms: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Freezer {
    pub fn start(mem: Arc<dyn Memory>, interval_ms: u64) -> Self {
        let entries: Entries = Default::default();
        let interval = Arc::new(AtomicU64::new(interval_ms.max(10)));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new().name("freezer".into()).spawn({
            let (entries, interval, stop) = (entries.clone(), interval.clone(), stop.clone());
            move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(interval.load(Ordering::Relaxed)));
                    let mut map = entries.lock().unwrap_or_else(|e| e.into_inner());
                    map.retain(|key, f| {
                        if let Some((start, offsets)) = &f.chain {
                            match pointer::resolve(mem.as_ref(), *start, offsets) {
                                Ok(a) => f.address = a,
                                // P. ex. al menú principal el jugador encara no existeix: s'espera
                                Err(_) => return true,
                            }
                        }
                        match mem.write_bytes(f.address, &f.bytes, WriteMode::Data) {
                        Ok(()) => {
                            f.failures = 0;
                            true
                        }
                        Err(e) => {
                            f.failures += 1;
                            if f.failures >= MAX_FAILURES {
                                tracing::warn!(key, error = %e, "es deixa de congelar: l'escriptura falla repetidament");
                            }
                            f.failures < MAX_FAILURES
                        }
                        }
                    });
                }
            }
        });
        Self { entries, interval_ms: interval, stop, thread: thread.ok() }
    }

    /// Congela (o actualitza) un valor. La primera escriptura la fa qui crida, per poder
    /// retornar l'error a la UI de seguida.
    pub fn set(&self, key: &str, address: u64, bytes: Vec<u8>) {
        self.lock().insert(key.to_string(), Frozen { address, chain: None, bytes, failures: 0 });
    }

    /// Com [`set`](Self::set), però recorrent la cadena de punters a cada cicle: si el joc
    /// mou l'objecte (carrega una partida), la congelació el segueix.
    pub fn set_chain(&self, key: &str, start: u64, offsets: Vec<i64>, address: u64, bytes: Vec<u8>) {
        self.lock().insert(key.to_string(), Frozen { address, chain: Some((start, offsets)), bytes, failures: 0 });
    }

    pub fn remove(&self, key: &str) -> bool {
        self.lock().remove(key).is_some()
    }

    pub fn keys(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    pub fn set_interval(&self, ms: u64) {
        self.interval_ms.store(ms.max(10), Ordering::Relaxed);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Frozen>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Freezer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::fake::FakeMemory;

    fn wait_until(cond: impl Fn() -> bool) -> bool {
        (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            cond()
        })
    }

    #[test]
    fn rewrites_value_after_game_changes_it() {
        let mem = Arc::new(FakeMemory::new(0x10000, 0x100, 8));
        let read = |m: &FakeMemory| {
            let mut b = [0u8; 4];
            m.read_bytes(0x10010, &mut b).unwrap();
            i32::from_le_bytes(b)
        };
        let f = Freezer::start(mem.clone(), 10);
        f.set("vida", 0x10010, 100i32.to_le_bytes().to_vec());
        assert!(wait_until(|| read(&mem) == 100), "la congelació escriu el valor");
        mem.put(0x10010, &37i32.to_le_bytes()); // el "joc" ens treu vida
        assert!(wait_until(|| read(&mem) == 100), "i el torna a posar");
        assert!(f.remove("vida"));
        mem.put(0x10010, &37i32.to_le_bytes());
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(read(&mem), 37, "descongelat ja no el toca");
    }

    #[test]
    fn drops_entries_that_keep_failing() {
        let mem = Arc::new(FakeMemory::new(0x10000, 0x100, 8));
        let f = Freezer::start(mem, 10);
        f.set("dolent", 0x90000, vec![1]);
        assert!(wait_until(|| f.keys().is_empty()));
    }
}
