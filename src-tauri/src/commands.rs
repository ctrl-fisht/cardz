use crate::ai::AiRuntime;
use crate::config;
use crate::model::*;
use crate::sync::SyncRuntime;
use crate::vault::Vault;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    pub vault: Mutex<Option<Vault>>,
    pub pending_import: Mutex<Option<PathBuf>>,
    pub sync: Mutex<SyncRuntime>,
    pub ai: tokio::sync::Mutex<AiRuntime>,
}

#[tauri::command]
pub fn init_vault(app: AppHandle, state: State<'_, AppState>) -> Result<VaultInfo, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    if let Some(vault) = guard.as_ref() {
        return vault.info();
    }

    let configured = config::read_config(&app)
        .vault
        .map(PathBuf::from)
        .filter(|path| path.is_dir());

    let root = match configured {
        Some(path) => path,
        None => {
            let documents = app
                .path()
                .document_dir()
                .map_err(|e| format!("Не удалось найти папку документов: {}", e))?;
            documents.join("Cardz")
        }
    };

    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut vault = Vault::open(root.clone())?;
    vault.seed_demo()?;
    let info = vault.info()?;
    *guard = Some(vault);
    drop(guard);
    config::save_vault(&app, &root)?;
    Ok(info)
}

#[tauri::command]
pub fn open_vault(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<VaultInfo, String> {
    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err("Папка не найдена".to_string());
    }
    let vault = Vault::open(root.clone())?;
    let info = vault.info()?;
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    *guard = Some(vault);
    drop(guard);
    config::save_vault(&app, &root)?;
    Ok(info)
}

#[tauri::command]
pub fn vault_info(state: State<'_, AppState>) -> Result<VaultInfo, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.info()
}

#[tauri::command]
pub fn sync_vault(state: State<'_, AppState>) -> Result<VaultInfo, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.sync()?;
    vault.info()
}

#[tauri::command]
pub fn create_deck(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Result<CreateDeckResult, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    let deck_id = vault.create_deck(&name, description.as_deref().unwrap_or(""))?;
    let vault_info = vault.info()?;
    Ok(CreateDeckResult {
        vault: vault_info,
        deck_id,
    })
}

#[tauri::command]
pub fn create_group(
    state: State<'_, AppState>,
    name: String,
) -> Result<CreateGroupResult, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    let group_id = vault.create_group(&name)?;
    let vault_info = vault.info()?;
    Ok(CreateGroupResult {
        vault: vault_info,
        group_id,
    })
}

#[tauri::command]
pub fn delete_group(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<VaultInfo, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.delete_group(&group_id)?;
    vault.info()
}

#[tauri::command]
pub fn delete_deck(
    state: State<'_, AppState>,
    deck_id: String,
) -> Result<VaultInfo, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.delete_deck(&deck_id)?;
    vault.info()
}

#[tauri::command]
pub fn move_deck(
    state: State<'_, AppState>,
    deck_id: String,
    target_group: String,
) -> Result<CreateDeckResult, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    let new_id = vault.move_deck(&deck_id, &target_group)?;
    let vault_info = vault.info()?;
    Ok(CreateDeckResult {
        vault: vault_info,
        deck_id: new_id,
    })
}

#[tauri::command]
pub fn export_deck(
    state: State<'_, AppState>,
    deck_id: String,
    dest_path: String,
) -> Result<(), String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.export_deck(&deck_id, Path::new(&dest_path))
}

#[tauri::command]
pub fn import_deck(
    state: State<'_, AppState>,
    source_path: String,
) -> Result<CreateDeckResult, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    let deck_id = vault.import_deck(Path::new(&source_path))?;
    let vault_info = vault.info()?;
    Ok(CreateDeckResult {
        vault: vault_info,
        deck_id,
    })
}

#[tauri::command]
pub fn startup_import(state: State<'_, AppState>) -> Result<Option<CreateDeckResult>, String> {
    let path = {
        let mut pending = state
            .pending_import
            .lock()
            .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
        pending.take()
    };
    let Some(path) = path else {
        return Ok(None);
    };
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let Some(vault) = guard.as_mut() else {
        if let Ok(mut pending) = state.pending_import.lock() {
            *pending = Some(path);
        }
        return Err("Хранилище не открыто".to_string());
    };
    let deck_id = vault.import_deck(&path)?;
    let vault_info = vault.info()?;
    Ok(Some(CreateDeckResult {
        vault: vault_info,
        deck_id,
    }))
}

#[tauri::command]
pub fn list_templates(
    state: State<'_, AppState>,
    deck_id: String,
) -> Result<Vec<Template>, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    let mut templates: Vec<Template> = vault
        .load_templates(&deck_id)?
        .into_values()
        .collect();
    templates.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(templates)
}

#[tauri::command]
pub fn list_notes(
    state: State<'_, AppState>,
    deck_id: String,
    query: Option<String>,
) -> Result<Vec<NoteSummary>, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.list_notes(&deck_id, query.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn get_note(
    state: State<'_, AppState>,
    deck_id: String,
    note_id: String,
) -> Result<NoteDetail, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.get_note(&deck_id, &note_id)
}

#[tauri::command]
pub fn save_note(state: State<'_, AppState>, input: SaveNoteInput) -> Result<NoteDetail, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.save_note(input)
}

#[tauri::command]
pub fn delete_note(
    state: State<'_, AppState>,
    deck_id: String,
    note_id: String,
) -> Result<(), String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.delete_note(&deck_id, &note_id)
}

#[tauri::command]
pub fn import_media(
    state: State<'_, AppState>,
    deck_id: String,
    source_path: String,
) -> Result<String, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.import_media(&deck_id, Path::new(&source_path))
}

#[tauri::command]
pub fn read_media(
    state: State<'_, AppState>,
    deck_id: String,
    path: String,
) -> Result<String, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.read_media(&deck_id, &path)
}

#[tauri::command]
pub fn next_card(
    state: State<'_, AppState>,
    scope: ReviewScope,
    include_future: bool,
    exclude: Vec<String>,
) -> Result<Option<ReviewCard>, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.next_card(&scope, include_future, &exclude)
}

#[tauri::command]
pub fn answer_card(
    state: State<'_, AppState>,
    card_id: String,
    rating: u32,
) -> Result<AnswerResult, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.answer_card(&card_id, rating)
}
