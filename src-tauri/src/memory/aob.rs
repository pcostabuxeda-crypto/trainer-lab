//! AOB scanning (Array Of Bytes): buscar una seqüència de bytes coneguda dins la memòria.
//!
//! Quan un joc s'actualitza, les adreces canvien, però el codi que llegeix la vida sovint
//! és idèntic o gairebé. Un patró com `48 8B 05 ?? ?? ?? ?? 48 85 C0` identifica aquest
//! codi: els `??` són comodins per als bytes que varien (adreces, offsets).

use std::str::FromStr;

use super::{Memory, Region};
use crate::error::{AppError, AppResult};

/// Mínim de bytes fixos perquè el patró sigui prou específic.
const MIN_FIXED: usize = 4;
/// Llegim la memòria a blocs d'aquesta mida.
const CHUNK: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern(Vec<Option<u8>>);

impl FromStr for Pattern {
    type Err = AppError;

    fn from_str(s: &str) -> AppResult<Self> {
        let bytes = s
            .split_whitespace()
            .map(|t| match t {
                "?" | "??" => Ok(None),
                _ if t.len() == 2 => u8::from_str_radix(t, 16)
                    .map(Some)
                    .map_err(|_| AppError::InvalidPattern(format!("«{t}» no és un byte hex"))),
                _ => Err(AppError::InvalidPattern(format!("«{t}» ha de tenir 2 xifres hex o ser ??"))),
            })
            .collect::<AppResult<Vec<_>>>()?;
        let fixed = bytes.iter().filter(|b| b.is_some()).count();
        if fixed < MIN_FIXED {
            return Err(AppError::InvalidPattern(format!("cal un mínim de {MIN_FIXED} bytes fixos (n'hi ha {fixed})")));
        }
        if bytes.first().is_some_and(Option::is_none) {
            return Err(AppError::InvalidPattern("no pot començar amb ?? (la coincidència seria ambigua)".into()));
        }
        Ok(Pattern(bytes))
    }
}

impl Pattern {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Posicions de totes les coincidències dins `hay`.
    pub fn find_all(&self, hay: &[u8]) -> Vec<usize> {
        if hay.len() < self.len() {
            return Vec::new();
        }
        // Cerca directa: provem cada posició. Prou ràpid per a mòduls de centenars de MB.
        (0..=hay.len() - self.len())
            .filter(|&i| self.0.iter().zip(&hay[i..]).all(|(p, b)| p.is_none_or(|p| p == *b)))
            .collect()
    }
}

/// Busca el patró a les regions llegibles entre `start` i `end`. Retorna adreces absolutes,
/// com a molt `max` coincidències.
pub fn scan(mem: &dyn Memory, pattern: &Pattern, start: u64, end: u64, max: usize) -> Vec<u64> {
    scan_chunked(mem, pattern, start, end, max, CHUNK)
}

fn scan_chunked(mem: &dyn Memory, pattern: &Pattern, start: u64, end: u64, max: usize, chunk: usize) -> Vec<u64> {
    let mut found = Vec::new();
    let readable = |r: &Region| r.committed && r.readable && !r.guard;
    for r in mem.regions(start, end).iter().filter(|r| readable(r)) {
        let (from, to) = (r.base.max(start), r.end().min(end));
        let mut pos = from;
        while pos < to {
            // Els blocs es solapen `len - 1` bytes perquè no se'ns escapi un patró partit en dos
            let len = (chunk as u64).min(to - pos) as usize;
            let mut buf = vec![0u8; len];
            if mem.read_bytes(pos, &mut buf).is_ok() {
                for i in pattern.find_all(&buf) {
                    let addr = pos + i as u64;
                    if found.last() != Some(&addr) {
                        found.push(addr);
                        if found.len() >= max {
                            return found;
                        }
                    }
                }
            }
            if pos + len as u64 >= to {
                break;
            }
            pos += (len - (pattern.len() - 1).min(len - 1)) as u64;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::fake::FakeMemory;

    fn p(s: &str) -> Pattern {
        s.parse().unwrap()
    }

    #[test]
    fn parses_patterns_and_rejects_bad_ones() {
        assert_eq!(p("48 8B ?? 05 06").0, vec![Some(0x48), Some(0x8B), None, Some(0x05), Some(0x06)]);
        assert!("48 8B 05 06 ??".parse::<Pattern>().is_ok(), "pot acabar amb comodins (típic amb rip)");
        assert_eq!(p("aa ? bb cc dd"), p("AA ?? BB CC DD"));
        for bad in ["", "48 8B", "48 ?? ?? ??", "?? 48 8B 05 06", "48 8B 5 06", "48 8B GG 06", "488B0506"] {
            assert!(bad.parse::<Pattern>().is_err(), "hauria de fallar: «{bad}»");
        }
    }

    #[test]
    fn finds_at_start_middle_end_and_with_wildcards() {
        let hay = [1, 2, 3, 4, 9, 1, 2, 7, 4, 9, 1, 2, 3, 4];
        assert_eq!(p("01 02 ?? 04 09").find_all(&hay), vec![0, 5]);
        assert_eq!(p("01 02 03 04").find_all(&hay), vec![0, 10]);
        assert!(p("05 06 07 08").find_all(&hay).is_empty());
        assert!(p("01 02 03 04").find_all(&hay[..3]).is_empty());
    }

    #[test]
    fn scan_finds_pattern_split_across_chunks() {
        let m = FakeMemory::new(0x10000, 64, 8);
        m.put(0x1001E, &[0xDE, 0xAD, 0xBE, 0xEF, 0x01]); // creua la frontera dels blocs de 32 bytes
        m.put(0x10030, &[0xDE, 0xAD, 0x00, 0xEF, 0x01]);
        let pat = p("DE AD ?? EF 01");
        assert_eq!(scan_chunked(&m, &pat, 0x10000, 0x10040, 10, 32), vec![0x1001E, 0x10030]);
        assert_eq!(scan_chunked(&m, &pat, 0x10000, 0x10040, 1, 32), vec![0x1001E], "respecta `max`");
        assert_eq!(scan_chunked(&m, &pat, 0x10020, 0x10040, 10, 32), vec![0x10030], "respecta l'inici");
    }
}
