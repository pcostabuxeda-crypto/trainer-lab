//! Comandes que el frontend pot cridar amb `invoke`. Han de ser primes: validen
//! l'entrada i deleguen als mòduls.

use crate::error::AppResult;
use crate::logging::{self, LogEntry, LogLevel};
use crate::memory::process::{self, ProcessInfo};

#[tauri::command]
pub async fn list_processes() -> AppResult<Vec<ProcessInfo>> {
    // async: Tauri l'executa fora del fil principal i la UI no es bloqueja
    let list = process::list_processes()?;
    tracing::debug!(count = list.len(), "processos llistats");
    Ok(list)
}

#[tauri::command]
pub fn get_logs(limit: Option<usize>, min_level: Option<LogLevel>) -> Vec<LogEntry> {
    logging::recent(limit.unwrap_or(500).min(2000), min_level.unwrap_or(LogLevel::Trace))
}
