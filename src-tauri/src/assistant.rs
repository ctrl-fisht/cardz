use crate::ai::AiCardProposal;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssistantChat {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub opencode_session_id: Option<String>,
    #[serde(default)]
    pub last_deck_id: Option<String>,
    #[serde(default)]
    pub messages: Vec<AssistantMessage>,
    #[serde(default)]
    pub pending: Option<PendingCards>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub role: String,
    pub content: String,
    pub at: i64,
    #[serde(default)]
    pub batch: Option<CardBatch>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CardBatch {
    pub id: String,
    pub deck_id: String,
    pub deck_name: String,
    pub note_ids: Vec<String>,
    pub cards: Vec<AiCardProposal>,
    pub created_at: i64,
    #[serde(default)]
    pub undone: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PendingCards {
    pub id: String,
    pub cards: Vec<AiCardProposal>,
    #[serde(default)]
    pub suggested_deck_id: Option<String>,
    #[serde(default)]
    pub question: String,
}

pub fn new_chat() -> AssistantChat {
    let now = crate::vault::now_ms();
    AssistantChat {
        id: ulid::Ulid::generate().to_string(),
        created_at: now,
        updated_at: now,
        opencode_session_id: None,
        last_deck_id: None,
        messages: Vec::new(),
        pending: None,
    }
}

fn chat_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Не удалось найти папку данных: {}", e))?
        .join("assistant");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("current.json"))
}

pub fn load(app: &AppHandle) -> Result<Option<AssistantChat>, String> {
    let path = chat_path(app)?;
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("Не удалось прочитать чат ассистента: {}", e))
}

pub fn load_or_new(app: &AppHandle) -> Result<AssistantChat, String> {
    Ok(load(app)?.unwrap_or_else(new_chat))
}

pub fn save(app: &AppHandle, chat: &AssistantChat) -> Result<(), String> {
    let path = chat_path(app)?;
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(chat).map_err(|e| e.to_string())?;
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}
