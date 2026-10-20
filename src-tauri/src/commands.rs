use crate::model::*;
use crate::vault::Vault;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    pub vault: Mutex<Option<Vault>>,
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    vault: Option<String>,
}

fn read_config(app: &AppHandle) -> Config {
    let Ok(dir) = app.path().app_config_dir() else {
        return Config::default();
    };
    fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_config(app: &AppHandle, root: &Path) -> Result<(), String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Не удалось найти папку конфигурации: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let config = Config {
        vault: Some(root.to_string_lossy().to_string()),
    };
    fs::write(
        dir.join("config.json"),
        serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
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

    let configured = read_config(&app)
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
    save_config(&app, &root)?;
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
    save_config(&app, &root)?;
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
) -> Result<VaultInfo, String> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_mut()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.create_deck(&name, description.as_deref().unwrap_or(""))?;
    vault.info()
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
    deck_id: String,
) -> Result<Option<ReviewCard>, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "Хранилище не открыто".to_string())?;
    vault.next_card(&deck_id)
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
