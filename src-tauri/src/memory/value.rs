//! Tipus de dada d'un valor en memòria i conversió a/des de bytes (little-endian,
//! que és l'ordre dels bytes a x86/x64).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ValueType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    /// 1 byte: 0 = fals, qualsevol altre valor = cert
    Bool,
    /// System.Decimal de .NET (16 bytes): jocs Unity/C# que guarden diners sense errors d'arrodoniment
    Decimal,
    /// Seqüència de bytes en cru (només per a patches de codi)
    Bytes,
}

impl ValueType {
    pub fn size(self) -> usize {
        use ValueType::*;
        match self {
            I8 | U8 | Bool => 1,
            I16 | U16 => 2,
            I32 | U32 | F32 => 4,
            I64 | U64 | F64 => 8,
            Decimal => 16,
            Bytes => 0,
        }
    }

    pub fn is_numeric(self) -> bool {
        !matches!(self, ValueType::Bool | ValueType::Bytes)
    }

    // shortcut: els valors viatgen com f64; els enters de més de 2^53 perden precisió.
    // Si mai cal, canviar a un enum de valor tipat.
    pub fn encode(self, v: f64) -> AppResult<Vec<u8>> {
        use ValueType::*;
        if self == Bytes {
            return Err(AppError::InvalidValue("el tipus bytes no té valor numèric".into()));
        }
        if !v.is_finite() {
            return Err(AppError::InvalidValue(format!("{v} no és un número finit")));
        }
        let int = |min: f64, max: f64| -> AppResult<f64> {
            if v.fract() != 0.0 {
                return Err(AppError::InvalidValue(format!("{v} no és un enter (tipus {self:?})")));
            }
            if v < min || v > max {
                return Err(AppError::InvalidValue(format!("{v} està fora del rang de {self:?} ({min}..={max})")));
            }
            Ok(v)
        };
        Ok(match self {
            I8 => (int(i8::MIN as f64, i8::MAX as f64)? as i8).to_le_bytes().to_vec(),
            I16 => (int(i16::MIN as f64, i16::MAX as f64)? as i16).to_le_bytes().to_vec(),
            I32 => (int(i32::MIN as f64, i32::MAX as f64)? as i32).to_le_bytes().to_vec(),
            I64 => (int(i64::MIN as f64, i64::MAX as f64)? as i64).to_le_bytes().to_vec(),
            U8 => (int(0.0, u8::MAX as f64)? as u8).to_le_bytes().to_vec(),
            U16 => (int(0.0, u16::MAX as f64)? as u16).to_le_bytes().to_vec(),
            U32 => (int(0.0, u32::MAX as f64)? as u32).to_le_bytes().to_vec(),
            U64 => (int(0.0, u64::MAX as f64)? as u64).to_le_bytes().to_vec(),
            F32 => {
                if v.abs() > f32::MAX as f64 {
                    return Err(AppError::InvalidValue(format!("{v} no cap en un f32")));
                }
                (v as f32).to_le_bytes().to_vec()
            }
            F64 => v.to_le_bytes().to_vec(),
            Decimal => encode_decimal(v)?,
            Bool => vec![u8::from(v != 0.0)],
            Bytes => unreachable!(),
        })
    }

    pub fn decode(self, b: &[u8]) -> AppResult<f64> {
        use ValueType::*;
        if self == Bytes || b.len() != self.size() {
            return Err(AppError::InvalidValue(format!("{self:?} necessita {} bytes, n'hi ha {}", self.size(), b.len())));
        }
        // La mida ja està comprovada: els `try_into` no poden fallar
        Ok(match self {
            I8 => b[0] as i8 as f64,
            I16 => i16::from_le_bytes(b.try_into().unwrap()) as f64,
            I32 => i32::from_le_bytes(b.try_into().unwrap()) as f64,
            I64 => i64::from_le_bytes(b.try_into().unwrap()) as f64,
            U8 => b[0] as f64,
            U16 => u16::from_le_bytes(b.try_into().unwrap()) as f64,
            U32 => u32::from_le_bytes(b.try_into().unwrap()) as f64,
            U64 => u64::from_le_bytes(b.try_into().unwrap()) as f64,
            F32 => f32::from_le_bytes(b.try_into().unwrap()) as f64,
            F64 => f64::from_le_bytes(b.try_into().unwrap()),
            Bool => f64::from(u8::from(b[0] != 0)),
            Decimal => decode_decimal(b).ok_or_else(|| AppError::InvalidValue("no és un decimal vàlid".into()))?,
            Bytes => unreachable!(),
        })
    }
}

/// System.Decimal: `flags` (escala als bits 16-23, signe al 31; la resta, zero), `hi` u32 i
/// `lo` u64. Valor = (hi·2⁶⁴ + lo) / 10^escala.
pub fn decode_decimal(b: &[u8]) -> Option<f64> {
    let flags = u32::from_le_bytes(b[0..4].try_into().ok()?);
    let scale = (flags >> 16) & 0xFF;
    if flags & 0x7F00_FFFF != 0 || scale > 28 {
        return None;
    }
    let hi = u32::from_le_bytes(b[4..8].try_into().ok()?) as u128;
    let lo = u64::from_le_bytes(b[8..16].try_into().ok()?) as u128;
    let v = ((hi << 64) | lo) as f64 / 10f64.powi(scale as i32);
    Some(if flags >> 31 == 1 { -v } else { v })
}

/// Amb els decimals mínims que representen `v` (999 → escala 0; 24,7 → 247 i escala 1).
fn encode_decimal(v: f64) -> AppResult<Vec<u8>> {
    if v.abs() >= 7.9e28 {
        return Err(AppError::InvalidValue(format!("{v} no cap en un decimal")));
    }
    // `{}` dona la forma més curta que torna al mateix f64 (24,7 i no 24,69999…), mai amb exponent
    let text = v.abs().to_string();
    let (int, frac) = text.split_once('.').unwrap_or((&text, ""));
    // Màxim 28 decimals i 96 bits de mantissa: si no hi cap, se'n treuen (com fa .NET)
    let mut frac = frac[..frac.len().min(28)].to_string();
    let mantissa = loop {
        match format!("{int}{frac}").parse::<u128>() {
            Ok(m) if m < 1 << 96 => break m,
            _ if !frac.is_empty() => { frac.pop(); }
            _ => return Err(AppError::InvalidValue(format!("{v} no cap en un decimal"))),
        }
    };
    let flags = (frac.len() as u32) << 16 | if v < 0.0 { 1 << 31 } else { 0 };
    let mut out = flags.to_le_bytes().to_vec();
    out.extend(((mantissa >> 64) as u32).to_le_bytes());
    out.extend((mantissa as u64).to_le_bytes());
    Ok(out)
}

/// Bytes hex separats per espais, sense comodins: `"90 90 90"`.
pub fn parse_hex_bytes(s: &str) -> AppResult<Vec<u8>> {
    let bytes = s
        .split_whitespace()
        .map(|t| {
            (t.len() == 2)
                .then(|| u8::from_str_radix(t, 16).ok())
                .flatten()
                .ok_or_else(|| AppError::InvalidValue(format!("«{t}» no és un byte hex")))
        })
        .collect::<AppResult<Vec<u8>>>()?;
    if bytes.is_empty() {
        return Err(AppError::InvalidValue("cal com a mínim un byte".into()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::ValueType::*;

    #[test]
    fn roundtrip_every_type() {
        for (t, v) in [(I8, -5.0), (I16, -300.0), (I32, 100.0), (I64, -9e15), (U8, 255.0), (U16, 65535.0), (U32, 4e9), (U64, 1e15), (F32, 1.5), (F64, -2.25), (Decimal, -24.7), (Decimal, 999.0), (Bool, 1.0)] {
            let bytes = t.encode(v).unwrap();
            assert_eq!(bytes.len(), t.size(), "{t:?}");
            assert_eq!(t.decode(&bytes).unwrap(), v, "{t:?}");
        }
        assert_eq!(I32.encode(100.0).unwrap(), vec![100, 0, 0, 0], "little-endian");
    }

    #[test]
    fn rejects_out_of_range_and_fractions() {
        assert!(U8.encode(256.0).is_err());
        assert!(U32.encode(-1.0).is_err());
        assert!(I32.encode(1.5).is_err());
        assert!(F32.encode(f64::NAN).is_err());
        assert!(F32.encode(1e300).is_err());
        assert!(I32.decode(&[1, 2]).is_err());
        assert!(Bytes.encode(1.0).is_err());
        assert!(Decimal.encode(1e30).is_err());
        assert!(Decimal.decode(&[1; 16]).is_err(), "bits reservats");
    }

    #[test]
    fn decimal_layout() {
        // 24,70093609495393900000000000 tal com el desa Moon Sweepers (escala 27)
        let b = [0, 0, 0x1b, 0, 0xac, 0x22, 0xd0, 0x4f, 0, 0x30, 0x1c, 0xcc, 0x5e, 0xe3, 0x27, 0xd4];
        assert!((Decimal.decode(&b).unwrap() - 24.7009).abs() < 1e-4);
        assert_eq!(Decimal.encode(999.0).unwrap(), [0, 0, 0, 0, 0, 0, 0, 0, 0xe7, 3, 0, 0, 0, 0, 0, 0]);
        assert_eq!(Decimal.encode(24.7).unwrap()[..4], [0, 0, 1, 0], "escala 1");
    }

    #[test]
    fn hex_bytes() {
        assert_eq!(super::parse_hex_bytes("90 90 ff").unwrap(), vec![0x90, 0x90, 0xFF]);
        for bad in ["", "9", "90 ??", "zz"] {
            assert!(super::parse_hex_bytes(bad).is_err(), "{bad}");
        }
    }
}
