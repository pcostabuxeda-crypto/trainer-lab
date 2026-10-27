mod commands;
mod error;
mod logging;
mod memory;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let log_dir = app.path().app_log_dir()?;
            std::fs::create_dir_all(&log_dir)?;
            logging::init(app.handle(), &log_dir);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), log_dir = %log_dir.display(), "Trainer Lab iniciat");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::list_processes, commands::get_logs])
        .run(tauri::generate_context!())
        .expect("error en executar l'aplicació Tauri");
}
