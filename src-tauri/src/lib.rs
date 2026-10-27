#[cfg(not(windows))]
compile_error!("Trainer Lab només funciona a Windows (fa servir l'API Win32 per accedir a la memòria).");

pub mod anticheat;
mod commands;
pub mod error;
pub mod games;
mod hotkeys;
mod logging;
pub mod memory;
mod session;
mod settings;
pub mod trainers;

use std::path::PathBuf;

use tauri::Manager;

/// Carpeta amb les definicions d'exemple. En desenvolupament és la del repositori
/// (així el hot-reload funciona editant `trainers/`); en producció, la dels recursos.
fn bundled_trainers_dir(app: &tauri::App) -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../trainers"))
    } else {
        app.path().resource_dir().map(|d| d.join("trainers")).unwrap_or_default()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(hotkeys::on_shortcut)
                .build(),
        )
        .manage(session::SessionState::default())
        .manage(hotkeys::HotkeyMap::default())
        .manage(games::GameStore::default())
        .setup(|app| {
            let log_dir = app.path().app_log_dir()?;
            std::fs::create_dir_all(&log_dir)?;
            logging::init(app.handle(), &log_dir);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), log_dir = %log_dir.display(), "Trainer Lab iniciat");

            let data_dir = app.path().app_data_dir()?;
            app.manage(settings::SettingsStore::load(data_dir.join("settings.json")));
            app.manage(trainers::TrainerStore::new(bundled_trainers_dir(app), data_dir.join("trainers")));
            trainers::watch(app.handle().clone());
            // Escaneja la biblioteca en segon pla i comença a vigilar els jocs en marxa
            games::watch_running(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_processes,
            commands::get_logs,
            commands::attach,
            commands::detach,
            commands::session_info,
            commands::list_trainers,
            commands::reload_trainers,
            commands::list_games,
            commands::running_games,
            commands::launch_game,
            commands::freeze_address,
            commands::add_manual_game,
            commands::remove_manual_game,
            commands::trainer_view,
            commands::cheat_states,
            commands::set_cheat,
            commands::set_hotkey,
            commands::scan_first,
            commands::scan_next,
            commands::scan_results,
            commands::scan_reset,
            commands::scan_write,
            commands::find_pointers,
            commands::save_cheat,
            commands::get_settings,
            commands::save_settings,
            commands::debug_read,
            commands::debug_write,
            commands::list_frozen,
            commands::unfreeze,
        ])
        .run(tauri::generate_context!())
        .expect("error en executar l'aplicació Tauri");
}
