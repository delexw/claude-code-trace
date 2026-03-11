#![allow(dead_code)]

mod parser;
mod commands;
mod convert;
mod watcher;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::session::load_session,
            commands::session::get_session_meta,
            commands::session::watch_session,
            commands::session::unwatch_session,
            commands::session::get_project_dirs,
            commands::picker::discover_sessions,
            commands::picker::watch_picker,
            commands::picker::unwatch_picker,
            commands::git::get_git_info,
            commands::debug::get_debug_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
