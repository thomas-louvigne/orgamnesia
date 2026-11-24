#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use orgamnesia_lib::{AppState, commands};

fn main() {
    if orgamnesia_lib::git::answer_askpass() { return; }
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("-h" | "--help") => {
            println!("Orgamnesia {}: a personal wiki in org-mode or Markdown

Usage:
  orgamnesia                   open the window
  orgamnesia -nw [folder]      open the project in the terminal (beta), else the last one
                               (also --tui, -t)
  orgamnesia -h, --help        show this help
  orgamnesia -V, --version     show the version", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some("-V" | "--version") => {
            println!("orgamnesia {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        // `-nw` (as Emacs), `--tui` or `-t` `[folder]`: the terminal interface (beta), without a window;
        // `--nox`, its name in 1.8, still works
        Some("-nw" | "--tui" | "-t" | "--nox") => {
            if let Err(e) = orgamnesia_lib::tui::run(args.next()) {
                eprintln!("orgamnesia: {e}");
                std::process::exit(1);
            }
            return;
        }
        _ => {}
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::new())
        .setup(|app| {
            use tauri::Manager;
            // Git and the dictionaries installed with the app (Windows)
            if let Ok(dir) = app.path().resource_dir() { orgamnesia_lib::set_resource_dir(dir); }
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
            commands::config::json_leftovers,
            commands::config::answer_json,
            commands::projects::open_vault,
            commands::projects::remove_project,
            commands::projects::existing_folders,
            commands::projects::check_disk,
            commands::projects::assets_offer,
            commands::projects::answer_assets,
            commands::pages::read_file,
            commands::pages::write_file,
            commands::pages::create_page,
            commands::pages::list_templates,
            commands::pages::list_pages,
            commands::pages::list_folders,
            commands::pages::create_folder,
            commands::pages::rename_folder,
            commands::pages::delete_folder,
            commands::pages::create_template,
            commands::links::get_backlinks,
            commands::links::get_broken_links,
            commands::links::get_graph,
            commands::links::list_tags,
            commands::links::search_tags,
            commands::links::list_todos,
            commands::links::get_agenda,
            commands::pages::rename_page,
            commands::pages::delete_page,
            commands::pages::restore_page,
            commands::pages::trash_count,
            commands::pages::empty_trash,
            commands::org_bloom::build_site,
            commands::org_bloom::site_config,
            commands::org_bloom::open_folder,
            commands::app::pick_folder,
            commands::app::import_file,
            commands::app::open_file_link,
            commands::git::git_status,
            commands::git::git_changes,
            commands::git::git_commit,
            commands::git::git_pull,
            commands::git::git_push,
            commands::git::git_set_passphrase,
            commands::git::git_fetch,
            commands::git::git_branches,
            commands::git::git_switch,
            commands::git::git_diff,
            commands::git::git_log,
            commands::git::git_commit_diff,
            commands::git::git_show,
            commands::git::git_init,
            commands::git::git_add_remote,
            commands::git::git_clone,
            commands::git::git_identity,
            commands::update::check_update,
            commands::update::install_update,
            commands::git::git_set_identity,
            commands::app::pick_file,
            commands::app::pick_image,
            commands::app::import_image,
            commands::app::missing_files,
            commands::app::read_image,
            commands::app::open_url,
            commands::theme::list_themes,
            commands::theme::read_theme,
            commands::theme::create_theme,
            commands::theme::open_themes_folder,
            commands::app::print_window,
            commands::app::set_fullscreen,
            commands::app::save_html,
            commands::app::quit_app,
            commands::clipboard::read_clipboard,
            commands::clipboard::write_clipboard,
            commands::emoji::show_emoji_picker,
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
