//! Comandes que el frontend pot cridar amb `invoke`. Han de ser primes: validen
//! l'entrada i deleguen als mòduls. Les `async` s'executen fora del fil principal.

use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use crate::error::AppResult;
use crate::logging::{self, LogEntry, LogLevel};
use crate::memory::locator::{self, Locator};
use crate::memory::process::{self, ProcessInfo};
use crate::memory::value::ValueType;
use crate::memory::{Memory, WriteMode};
use crate::session::{Session, SessionInfo, SessionState};

#[tauri::command]
pub async fn list_processes() -> AppResult<Vec<ProcessInfo>> {
    let list = process::list_processes()?;
    tracing::debug!(count = list.len(), "processos llistats");
    Ok(list)
}

#[tauri::command]
pub fn get_logs(limit: Option<usize>, min_level: Option<LogLevel>) -> Vec<LogEntry> {
    logging::recent(limit.unwrap_or(500).min(2000), min_level.unwrap_or(LogLevel::Trace))
}

#[tauri::command]
pub async fn attach(
    app: AppHandle,
    state: State<'_, SessionState>,
    pid: u32,
    acknowledge_warnings: Option<bool>,
) -> AppResult<SessionInfo> {
    state.attach(&app, pid, acknowledge_warnings.unwrap_or(false))
}

#[tauri::command]
pub fn detach(app: AppHandle, state: State<'_, SessionState>) -> bool {
    state.detach(&app, "Desadjuntat per l'usuari", None)
}

#[tauri::command]
pub fn session_info(state: State<'_, SessionState>) -> Option<SessionInfo> {
    state.info()
}

// --- Comandes de depuració (Fase 2). A la Fase 3 la UI passarà per `set_cheat`. ---

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryReading {
    /// Adreça resolta, en hex
    pub address: String,
    pub value: f64,
}

fn resolve_and_read(s: &Session, loc: &Locator, ty: ValueType) -> AppResult<(u64, f64)> {
    let address = locator::resolve(s.process.as_ref(), &s.modules, loc)?;
    let mut buf = vec![0u8; ty.size()];
    s.process.read_bytes(address, &mut buf)?;
    Ok((address, ty.decode(&buf)?))
}

#[tauri::command]
pub async fn debug_read(state: State<'_, SessionState>, locator: Locator, value_type: ValueType) -> AppResult<MemoryReading> {
    state.with(|s| {
        let (address, value) = resolve_and_read(s, &locator, value_type)?;
        Ok(MemoryReading { address: format!("{address:#x}"), value })
    })
}

#[tauri::command]
pub async fn debug_write(
    state: State<'_, SessionState>,
    locator: Locator,
    value_type: ValueType,
    value: f64,
    freeze: bool,
) -> AppResult<MemoryReading> {
    let bytes = value_type.encode(value)?;
    state.with(|s| {
        let address = locator::resolve(s.process.as_ref(), &s.modules, &locator)?;
        s.process.write_bytes(address, &bytes, WriteMode::Data)?;
        if freeze {
            s.freezer.set(&format!("{address:#x}:{value_type:?}"), address, bytes);
        }
        tracing::info!(address = format_args!("{address:#x}"), value, freeze, "valor escrit");
        // Tornem a llegir per confirmar el que ha quedat a la memòria
        let (_, value) = resolve_and_read(s, &locator, value_type)?;
        Ok(MemoryReading { address: format!("{address:#x}"), value })
    })
}

#[tauri::command]
pub fn list_frozen(state: State<'_, SessionState>) -> AppResult<Vec<String>> {
    state.with(|s| Ok(s.freezer.keys()))
}

#[tauri::command]
pub fn unfreeze(state: State<'_, SessionState>, key: String) -> AppResult<bool> {
    state.with(|s| Ok(s.freezer.remove(&key)))
}
