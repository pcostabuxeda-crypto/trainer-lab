//! Tipus de dada d'un valor en memòria i conversió a/des de bytes (little-endian,
//! que és l'ordre dels bytes a x86/x64).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
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
}

impl ValueType {
    pub fn size(self) -> usize {
        use ValueType::*;
        match self {
            I8 | U8 | Bool => 1,
            I16 | U16 => 2,
            I32 | U32 | F32 => 4,
            I64 | U64 | F64 => 8,
        }
    }

    // shortcut: els valors viatgen com f64; els enters de més de 2^53 perden precisió.
    // Si mai cal, canviar a un enum de valor tipat.
    pub fn encode(self, v: f64) -> AppResult<Vec<u8>> {
        use ValueType::*;
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
            Bool => vec![u8::from(v != 0.0)],
        })
    }

    pub fn decode(self, b: &[u8]) -> AppResult<f64> {
        use ValueType::*;
        if b.len() != self.size() {
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ValueType::*;

    #[test]
    fn roundtrip_every_type() {
        for (t, v) in [(I8, -5.0), (I16, -300.0), (I32, 100.0), (I64, -9e15), (U8, 255.0), (U16, 65535.0), (U32, 4e9), (U64, 1e15), (F32, 1.5), (F64, -2.25), (Bool, 1.0)] {
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
    }
}
