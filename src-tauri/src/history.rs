use crate::ai::{AiCardProposal, AiMessage, AiVerdict};
use crate::model::ReviewScope;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InterviewRecord {
    pub id: String,
    pub title: String,
    pub scope: ReviewScope,
    pub model: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub opencode_session_id: Option<String>,
    #[serde(default)]
    pub messages: Vec<AiMessage>,
    #[serde(default)]
    pub verdict: Option<AiVerdict>,
    #[serde(default)]
    pub cards: Vec<AiCardProposal>,
    #[serde(default)]
    pub cards_created: usize,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub duration_sec: i64,
    #[serde(default)]
    pub material_total: usize,
    #[serde(default)]
    pub material_included: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InterviewSummary {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: String,
    pub level: Option<String>,
    pub message_count: usize,
    pub cards_created: usize,
    pub model: String,
    pub duration_sec: i64,
}

fn history_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Не удалось найти папку данных: {}", e))?
        .join("interviews");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn safe_id(id: &str) -> Result<&str, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err("Некорректный идентификатор собеседования".to_string());
    }
    Ok(id)
}

fn record_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(history_dir(app)?.join(format!("{}.json", safe_id(id)?)))
}

pub fn save(app: &AppHandle, record: &InterviewRecord) -> Result<(), String> {
    let path = record_path(app, &record.id)?;
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn load(app: &AppHandle, id: &str) -> Result<InterviewRecord, String> {
    let path = record_path(app, id)?;
    let text = fs::read_to_string(path).map_err(|_| "Собеседование не найдено".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("Не удалось прочитать историю: {}", e))
}

pub fn list(app: &AppHandle) -> Result<Vec<InterviewSummary>, String> {
    let dir = history_dir(app)?;
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<InterviewRecord>(&text) else {
            continue;
        };
        out.push(InterviewSummary {
            id: record.id,
            title: record.title,
            created_at: record.created_at,
            updated_at: record.updated_at,
            status: record.status,
            level: record.verdict.as_ref().map(|verdict| verdict.level.clone()),
            message_count: record.messages.len(),
            cards_created: record.cards_created,
            model: record.model,
            duration_sec: record.duration_sec,
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

pub fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    let path = record_path(app, id)?;
    if path.is_file() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn clear(app: &AppHandle) -> Result<(), String> {
    let dir = history_dir(app)?;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}
