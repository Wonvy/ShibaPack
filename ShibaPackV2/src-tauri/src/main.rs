#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod icons;
mod models;
mod platform;
mod scanner;
mod state;
mod summary;

use state::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = AppState::new(app.handle().clone())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_settings,
            commands::update_settings,
            commands::get_library,
            commands::get_tags,
            commands::create_tag,
            commands::update_tag,
            commands::delete_tag,
            commands::assign_tags,
            commands::create_software,
            commands::update_software,
            commands::delete_software,
            commands::add_version,
            commands::delete_version,
            commands::set_default_version,
            commands::delete_file,
            commands::run_file,
            commands::reveal_file,
            commands::check_file,
            commands::scan_directory,
            commands::add_packages,
            commands::delete_package,
            commands::run_package,
            commands::reveal_package,
            commands::reveal_data_dir,
            commands::reveal_path,
            commands::write_text_file,
            commands::read_text_file,
            commands::fetch_summary,
            commands::get_installed_apps,
            commands::uninstall_app,
            commands::create_category,
            commands::update_category,
            commands::delete_category,
            commands::reorder_categories,
            commands::assign_category,
            commands::get_storage_info,
            commands::list_backups,
            commands::restore_backup,
            commands::verify_data,
            commands::export_backup,
            commands::import_backup,
            commands::get_software,
            commands::get_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ShibaPack");
}
