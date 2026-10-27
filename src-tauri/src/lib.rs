#[cfg(not(windows))]
compile_error!("Trainer Lab només funciona a Windows (fa servir l'API Win32 per accedir a la memòria).");

pub mod anticheat;
mod commands;
pub mod error;
mod logging;
pub mod memory;
mod session;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(session::SessionState::default())
        .setup(|app| {
            let log_dir = app.path().app_log_dir()?;
            std::fs::create_dir_all(&log_dir)?;
            logging::init(app.handle(), &log_dir);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), log_dir = %log_dir.display(), "Trainer Lab iniciat");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_processes,
            commands::get_logs,
            commands::attach,
            commands::detach,
            commands::session_info,
            commands::debug_read,
            commands::debug_write,
            commands::list_frozen,
            commands::unfreeze,
        ])
        .run(tauri::generate_context!())
        .expect("error en executar l'aplicació Tauri");
}
