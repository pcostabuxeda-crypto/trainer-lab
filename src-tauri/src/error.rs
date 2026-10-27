//! Error comú del backend. Es serialitza com `{ code, message }` perquè la UI pugui
//! reaccionar per `code` (estable) i mostrar `message` (text humà).

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Error del sistema: {0}")]
    Os(String),
    #[error(
        "Windows ha denegat l'accés al procés. Si el joc s'executa com a administrador, Trainer Lab també ho ha de fer."
    )]
    AccessDenied,
    #[error("No hi ha cap joc adjuntat.")]
    NotAttached,
    #[error("El procés del joc s'ha tancat.")]
    ProcessExited,
    #[error("Bloquejat: {0}")]
    AntiCheatBlocked(String),
    #[error("Avís d'anti-cheat: {0}")]
    AntiCheatWarning(String),
    #[error("No s'ha trobat el mòdul «{0}» al procés.")]
    ModuleNotFound(String),
    #[error("Patró AOB invàlid: {0}")]
    InvalidPattern(String),
    #[error("No s'ha trobat el patró AOB al mòdul «{0}».")]
    PatternNotFound(String),
    #[error("Punter nul al pas {step} de la cadena (adreça {address:#x}).")]
    NullPointer { step: usize, address: u64 },
    #[error("No s'ha pogut llegir {len} bytes a {address:#x}.")]
    ReadFailed { address: u64, len: usize },
    #[error("Escriptura rebutjada a {address:#x}: {reason}")]
    WriteRejected { address: u64, reason: String },
    #[error("Valor invàlid: {0}")]
    InvalidValue(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Os(_) => "OS_ERROR",
            AppError::AccessDenied => "ACCESS_DENIED",
            AppError::NotAttached => "NOT_ATTACHED",
            AppError::ProcessExited => "PROCESS_EXITED",
            AppError::AntiCheatBlocked(_) => "ANTICHEAT_BLOCKED",
            AppError::AntiCheatWarning(_) => "ANTICHEAT_WARNING",
            AppError::ModuleNotFound(_) => "MODULE_NOT_FOUND",
            AppError::InvalidPattern(_) => "INVALID_PATTERN",
            AppError::PatternNotFound(_) => "PATTERN_NOT_FOUND",
            AppError::NullPointer { .. } => "NULL_POINTER",
            AppError::ReadFailed { .. } => "READ_FAILED",
            AppError::WriteRejected { .. } => "WRITE_REJECTED",
            AppError::InvalidValue(_) => "INVALID_VALUE",
        }
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for AppError {
    fn from(e: windows::core::Error) -> Self {
        if e.code() == windows::Win32::Foundation::ERROR_ACCESS_DENIED.to_hresult() {
            AppError::AccessDenied
        } else {
            AppError::Os(e.message().to_string())
        }
    }
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ErrorPayload { code: self.code().into(), message: self.to_string() }.serialize(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;
