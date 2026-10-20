mod commands;
mod db;
mod model;
mod vault;

use commands::AppState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            vault: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_vault,
            commands::open_vault,
            commands::vault_info,
            commands::sync_vault,
            commands::create_deck,
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
