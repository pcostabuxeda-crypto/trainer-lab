//! Error comú del backend. Es serialitza com `{ code, message }` perquè la UI pugui
//! reaccionar per `code` (estable) i mostrar `message` (text humà).

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Error del sistema: {0}")]
    Os(String),
}

#[cfg(windows)]
impl From<windows::core::Error> for AppError {
    fn from(e: windows::core::Error) -> Self {
        AppError::Os(e.message().to_string())
    }
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let code = match self {
            AppError::Os(_) => "OS_ERROR",
        };
        ErrorPayload { code, message: self.to_string() }.serialize(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;
