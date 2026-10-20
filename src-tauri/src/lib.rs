mod commands;
mod db;
mod model;
mod vault;

use commands::AppState;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pending_import = std::env::args()
        .skip(1)
        .find(|arg| arg.to_lowercase().ends_with(".crdz"))
        .map(PathBuf::from);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            vault: Mutex::new(None),
            pending_import: Mutex::new(pending_import),
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_vault,
            commands::open_vault,
            commands::vault_info,
            commands::sync_vault,
            commands::create_deck,
            commands::create_group,
            commands::delete_group,
            commands::delete_deck,
            commands::move_deck,
            commands::export_deck,
            commands::import_deck,
            commands::startup_import,
            commands::list_templates,
            commands::list_notes,
            commands::get_note,
            commands::save_note,
            commands::delete_note,
            commands::import_media,
            commands::read_media,
            commands::next_card,
            commands::answer_card,
        ])
        .run(tauri::generate_context!())
        .expect("error while running cardz");
}
