use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_NOTE_TYPE: &str = "basic";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub root: String,
    pub name: String,
    pub decks: Vec<Deck>,
    pub groups: Vec<Group>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateDeckResult {
    pub vault: VaultInfo,
    pub deck_id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupResult {
    pub vault: VaultInfo,
    pub group_id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Deck {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub counts: Counts,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub new: i64,
    pub due: i64,
    pub learning: i64,
    pub total: i64,
    pub reviewed_today: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummary {
    pub id: String,
    pub deck_id: String,
    pub note_type: String,
    pub tags: Vec<String>,
    pub preview: String,
    pub modified: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NoteDetail {
    pub id: String,
    pub deck_id: String,
    pub path: String,
    pub note_type: String,
    pub tags: Vec<String>,
    pub fields: HashMap<String, String>,
    pub modified: i64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Template {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub cards: Vec<CardTemplate>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CardTemplate {
    pub name: String,
    pub front: String,
    pub back: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReviewScope {
    Deck { id: String },
    Group { id: String },
    All,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCard {
    pub card_id: String,
    pub note_id: String,
    pub deck_id: String,
    pub deck_name: String,
    pub deck_path: String,
    pub fields: HashMap<String, String>,
    pub tags: Vec<String>,
    pub card_name: String,
    pub front: String,
    pub back: String,
    pub state: i64,
    pub intervals: HashMap<String, String>,
    pub is_new: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerResult {
    pub due: i64,
    pub label: String,
    pub state: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveNoteInput {
    pub deck_id: String,
    pub note_id: Option<String>,
    pub note_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

#[derive(Deserialize, Serialize, Default)]
pub struct DeckMeta {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize, Serialize, Default)]
pub struct GroupMeta {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

pub struct AiMaterial {
    pub text: String,
    pub total_notes: usize,
    pub included_notes: usize,
}

#[derive(Deserialize, Default)]
pub struct FrontMatter {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub note_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
}
