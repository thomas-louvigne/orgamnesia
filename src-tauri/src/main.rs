#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use orgamnesia_lib::{AppState, commands};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            use tauri::Manager;
            if let Ok(dir) = app.path().app_config_dir() {
                orgamnesia_lib::settings::migrate_legacy_config(&dir);
                *app.state::<AppState>().settings.lock().unwrap() = orgamnesia_lib::settings::load(&dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::get_keybindings,
            commands::set_keybindings,
            commands::open_vault,
            commands::remove_project,
            commands::read_file,
            commands::write_file,
            commands::create_page,
            commands::get_backlinks,
            commands::get_broken_links,
            commands::list_tags,
            commands::search_tags,
            commands::list_todos,
            commands::rename_page,
            commands::delete_page,
            commands::restore_page,
            commands::trash_count,
            commands::empty_trash,
            commands::export_vault,
            commands::pick_folder,
            commands::git_status,
            commands::quit_app,
            commands::set_session,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                commands::on_exit(app);
            }
        });
}
