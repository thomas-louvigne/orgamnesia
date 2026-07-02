#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use org_wiki_flow_lib::{AppState, commands};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::get_keybindings,
            commands::set_keybindings,
            commands::open_vault,
            commands::list_files,
            commands::read_file,
            commands::write_file,
            commands::create_page,
            commands::get_backlinks,
            commands::rename_page,
            commands::export_vault,
            commands::pick_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
