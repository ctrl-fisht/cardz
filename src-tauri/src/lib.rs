mod ai;
mod assistant;
mod commands;
mod config;
mod db;
mod history;
mod model;
mod sync;
mod vault;

use ai::AiRuntime;
use commands::AppState;
use std::path::PathBuf;
use std::sync::Mutex;
use sync::SyncRuntime;
use tauri::Manager;

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
            sync: Mutex::new(SyncRuntime::default()),
            ai: tokio::sync::Mutex::new(AiRuntime::default()),
        })
        .setup(|app| {
            sync::spawn_auto_sync(app.handle().clone());
            Ok(())
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
            sync::sync_status,
            sync::sync_connect,
            sync::sync_now,
            sync::sync_pull,
            sync::sync_disconnect,
            sync::sync_resolve,
            sync::sync_abort,
            ai::ai_settings_get,
            ai::ai_settings_save,
            ai::ai_status,
            ai::ai_models,
            ai::ai_interview_start,
            ai::ai_interview_send,
            ai::ai_interview_verdict,
            ai::ai_interview_cards,
            ai::ai_interview_commit,
            ai::ai_assistant_generate,
            ai::ai_assistant_commit,
            ai::ai_assistant_chat_load,
            ai::ai_assistant_chat_new,
            ai::ai_assistant_chat_prepare,
            ai::ai_assistant_chat_send,
            ai::ai_assistant_chat_confirm,
            ai::ai_assistant_chat_cancel_pending,
            ai::ai_assistant_chat_undo,
            ai::ai_interview_abort,
            ai::ai_interview_resume,
            ai::ai_history_list,
            ai::ai_history_get,
            ai::ai_history_delete,
            ai::ai_history_clear,
            ai::ai_stop,
        ])
        .build(tauri::generate_context!())
        .expect("error while building cardz")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.ai.blocking_lock().stop_server();
                }
            }
        });
}
