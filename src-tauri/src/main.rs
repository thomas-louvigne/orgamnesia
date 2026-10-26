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
                let state = app.state::<AppState>();
                let settings = orgamnesia_lib::settings::load(&dir, &mut orgamnesia_lib::lock(&state.config));
                *orgamnesia_lib::lock(&state.settings) = settings;
                // Without watching, the files changed outside the app are only read on the next start
                *orgamnesia_lib::lock(&state.config_watcher) = orgamnesia_lib::config_files::watch(app.handle().clone(), dir).ok();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::get_settings,
            commands::config::set_settings,
            commands::config::get_keybindings,
            commands::config::set_keybindings,
            commands::config::config_files,
            commands::config::read_config,
            commands::config::apply_config,
            commands::config::config_versions,
            commands::config::restore_config,
            commands::config::config_problems,
            commands::projects::open_vault,
            commands::projects::remove_project,
            commands::projects::check_disk,
            commands::projects::assets_offer,
            commands::projects::answer_assets,
            commands::pages::read_file,
            commands::pages::write_file,
            commands::pages::create_page,
            commands::links::get_backlinks,
            commands::links::get_broken_links,
            commands::links::list_tags,
            commands::links::search_tags,
            commands::links::list_todos,
            commands::pages::rename_page,
            commands::pages::delete_page,
            commands::pages::restore_page,
            commands::pages::trash_count,
            commands::pages::empty_trash,
            commands::org_bloom::build_site,
            commands::org_bloom::site_config,
            commands::org_bloom::open_folder,
            commands::app::pick_folder,
            commands::git::git_status,
            commands::git::git_changes,
            commands::git::git_commit,
            commands::git::git_pull,
            commands::git::git_push,
            commands::app::pick_file,
            commands::app::pick_image,
            commands::app::import_image,
            commands::app::missing_files,
            commands::app::read_image,
            commands::app::open_url,
            commands::app::quit_app,
            commands::clipboard::read_clipboard,
            commands::clipboard::write_clipboard,
            commands::config::set_session,
            commands::search::search_project,
            commands::search::replace_in_project,
            commands::search::undo_replace,
            commands::search::can_undo_replace,
            commands::spell::spell_languages,
            commands::spell::spell_check,
            commands::spell::spell_suggest,
            commands::spell::spell_add,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                commands::app::on_exit(app);
            }
        });
}
