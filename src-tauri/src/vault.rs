use crate::db;
use crate::model::*;
use base64::Engine;
use fsrs::{FSRS, MemoryState};
use rusqlite::{params, params_from_iter, types::Value, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub struct Vault {
    pub root: PathBuf,
    pub conn: rusqlite::Connection,
    fsrs: FSRS,
}

pub struct DeckInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
}

pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
}

pub struct ParsedNote {
    pub id: String,
    pub note_type: String,
    pub tags: Vec<String>,
    pub fields: HashMap<String, String>,
    pub path: PathBuf,
    pub mtime: i64,
}

struct CardRow {
    card_id: String,
    note_id: String,
    deck_id: String,
    ord: i64,
    state: i64,
    stability: Option<f64>,
    difficulty: Option<f64>,
    last_review: Option<i64>,
    due: Option<i64>,
    lapses: i64,
}

impl Vault {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let meta_dir = root.join(".cardz");
        fs::create_dir_all(&meta_dir).map_err(|e| e.to_string())?;
        let conn = db::open(&meta_dir.join("index.db"))?;
        let mut vault = Self {
            root,
            conn,
            fsrs: FSRS::default(),
        };
        vault.sync()?;
        Ok(vault)
    }

    pub fn info(&self) -> Result<VaultInfo, String> {
        let decks = discover_decks(&self.root)?;
        let groups = discover_groups(&self.root, &decks);
        let now = now_ms();
        let day_start = db::day_start_ms();
        let mut out = Vec::with_capacity(decks.len());
        for d in decks {
            let counts = db::deck_counts(&self.conn, &d.id, now, day_start)?;
            out.push(Deck {
                id: d.id,
                name: d.name,
                description: d.description,
                path: d.dir.to_string_lossy().to_string(),
                counts,
            });
        }
        let group_out = groups
            .into_iter()
            .map(|group| Group {
                id: group.id,
                name: group.name,
                path: group.dir.to_string_lossy().to_string(),
            })
            .collect();
        let name = self
            .root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Cardz".to_string());
        Ok(VaultInfo {
            root: self.root.to_string_lossy().to_string(),
            name,
            decks: out,
            groups: group_out,
        })
    }

    pub fn deck_dir(&self, deck_id: &str) -> Result<PathBuf, String> {
        let dir = safe_join(&self.root, deck_id)?;
        if !dir.is_dir() {
            return Err(format!("Колода «{}» не найдена", deck_id));
        }
        Ok(dir)
    }

    pub fn deck_name(&self, deck_id: &str) -> String {
        self.deck_dir(deck_id)
            .ok()
            .map(|dir| {
                read_deck_meta(&dir)
                    .name
                    .unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().to_string())
            })
            .unwrap_or_else(|| deck_id.to_string())
    }

    pub fn load_templates(&self, deck_id: &str) -> Result<HashMap<String, Template>, String> {
        let dir = self.deck_dir(deck_id)?;
        Ok(load_templates(&dir))
    }

    pub fn list_notes(&self, deck_id: &str, query: &str) -> Result<Vec<NoteSummary>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT note_id, note_type, tags, preview, mtime
                 FROM notes WHERE deck_id = ?1
                 ORDER BY mtime DESC, note_id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![deck_id], |row| {
                let tags_json: String = row.get(2)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok(NoteSummary {
                    id: row.get(0)?,
                    deck_id: deck_id.to_string(),
                    note_type: row.get(1)?,
                    tags,
                    preview: row.get(3)?,
                    modified: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let needle = query.trim().to_lowercase();
        let mut notes = Vec::new();
        for row in rows {
            let note = row.map_err(|e| e.to_string())?;
            if needle.is_empty()
                || note.preview.to_lowercase().contains(&needle)
                || note.tags.iter().any(|t| t.to_lowercase().contains(&needle))
            {
                notes.push(note);
            }
        }
        Ok(notes)
    }

    pub fn get_note(&self, deck_id: &str, note_id: &str) -> Result<NoteDetail, String> {
        let rel: String = self
            .conn
            .query_row(
                "SELECT path FROM notes WHERE note_id = ?1 AND deck_id = ?2",
                params![note_id, deck_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Заметка не найдена. Выполните синхронизацию.".to_string())?;
        let path = safe_join(&self.root, &rel)?;
        let parsed = parse_note(&path)?;
        Ok(NoteDetail {
            id: parsed.id,
            deck_id: deck_id.to_string(),
            path: rel,
            note_type: parsed.note_type,
            tags: parsed.tags,
            fields: parsed.fields,
            modified: parsed.mtime,
        })
    }

    pub fn save_note(&mut self, input: SaveNoteInput) -> Result<NoteDetail, String> {
        let deck_dir = self.deck_dir(&input.deck_id)?;
        let templates = load_templates(&deck_dir);
        let template = templates
            .get(&input.note_type)
            .or_else(|| templates.get(DEFAULT_NOTE_TYPE))
            .cloned()
            .unwrap_or_else(builtin_template);

        let note_id = input
            .note_id
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| ulid::Ulid::generate().to_string());

        let fields: HashMap<String, String> = input
            .fields
            .into_iter()
            .map(|(k, v)| (k.trim().to_lowercase(), v))
            .filter(|(k, _)| !k.is_empty())
            .collect();

        let existing_rel: Option<String> = self
            .conn
            .query_row(
                "SELECT path FROM notes WHERE note_id = ?1",
                params![note_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        let path = if let Some(rel) = existing_rel {
            safe_join(&self.root, &rel)?
        } else {
            let notes_dir = deck_dir.join("notes");
            fs::create_dir_all(&notes_dir).map_err(|e| e.to_string())?;
            let front = fields
                .get("front")
                .cloned()
                .or_else(|| {
                    template
                        .fields
                        .first()
                        .and_then(|field| fields.get(field).cloned())
                })
                .unwrap_or_default();
            unique_note_path(&notes_dir, &note_slug(&front))
        };

        write_note_file(
            &path,
            &note_id,
            &input.note_type,
            &input.tags,
            &fields,
            &template.fields,
        )?;
        let rel_path = relative_string(&self.root, &path);
        self.sync()?;

        Ok(NoteDetail {
            id: note_id,
            deck_id: input.deck_id,
            path: rel_path,
            note_type: input.note_type,
            tags: input.tags,
            fields,
            modified: now_ms(),
        })
    }

    pub fn delete_note(&mut self, deck_id: &str, note_id: &str) -> Result<(), String> {
        let rel: Option<String> = self
            .conn
            .query_row(
                "SELECT path FROM notes WHERE note_id = ?1 AND deck_id = ?2",
                params![note_id, deck_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(rel) = rel {
            let path = safe_join(&self.root, &rel)?;
            if path.is_file() {
                fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
        }
        self.sync()
    }

    pub fn create_deck(&mut self, name: &str, description: &str) -> Result<String, String> {
        let raw = name.trim();
        if raw.is_empty() {
            return Err("Название колоды не может быть пустым".to_string());
        }
        let parts: Vec<&str> = raw
            .split(['/', '\\'])
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .collect();
        let Some(deck_label) = parts.last().copied() else {
            return Err("Название колоды не может быть пустым".to_string());
        };

        let mut parent = self.root.clone();
        let mut parent_ids: Vec<String> = Vec::new();
        for part in &parts[..parts.len() - 1] {
            let segment = sanitize_segment(part);
            parent = parent.join(&segment);
            parent_ids.push(segment);
            fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
            if !parent.join("deck.yaml").is_file() && !parent.join("group.yaml").is_file() {
                let meta = GroupMeta {
                    id: Some(parent_ids.join("/")),
                    name: Some(part.trim().to_string()),
                };
                fs::write(
                    parent.join("group.yaml"),
                    serde_yaml::to_string(&meta).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            }
        }

        let base = sanitize_segment(deck_label);
        let mut folder = base.clone();
        let mut n = 2;
        while parent.join(&folder).join("deck.yaml").is_file() {
            folder = format!("{}-{}", base, n);
            n += 1;
        }

        let dir = parent.join(&folder);
        fs::create_dir_all(dir.join("notes")).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("media")).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("templates")).map_err(|e| e.to_string())?;
        let stale_group = dir.join("group.yaml");
        if stale_group.is_file() {
            let _ = fs::remove_file(stale_group);
        }

        let mut ids = parent_ids;
        ids.push(folder.clone());
        let deck_id = ids.join("/");

        let meta = DeckMeta {
            id: Some(deck_id.clone()),
            name: Some(deck_label.to_string()),
            description: Some(description.trim().to_string()),
        };
        fs::write(
            dir.join("deck.yaml"),
            serde_yaml::to_string(&meta).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::write(dir.join("templates").join("basic.yaml"), BASIC_TEMPLATE_YAML)
            .map_err(|e| e.to_string())?;
        Ok(deck_id)
    }

    pub fn create_group(&mut self, name: &str) -> Result<String, String> {
        let raw = name.trim();
        if raw.is_empty() {
            return Err("Название группы не может быть пустым".to_string());
        }
        let parts: Vec<&str> = raw
            .split(['/', '\\'])
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            return Err("Название группы не может быть пустым".to_string());
        }

        let mut dir = self.root.clone();
        let mut ids: Vec<String> = Vec::new();
        for part in &parts {
            let segment = sanitize_segment(part);
            dir = dir.join(&segment);
            ids.push(segment);
            if !dir.exists() {
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            } else if dir.join("deck.yaml").is_file() {
                return Err(format!(
                    "Папка «{}» уже занята колодой",
                    ids.join("/")
                ));
            }
            let meta = GroupMeta {
                id: Some(ids.join("/")),
                name: Some(part.trim().to_string()),
            };
            fs::write(
                dir.join("group.yaml"),
                serde_yaml::to_string(&meta).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(ids.join("/"))
    }

    pub fn delete_group(&mut self, group_id: &str) -> Result<(), String> {
        let dir = safe_join(&self.root, group_id)?;
        if !dir.is_dir() {
            return Err("Папка не найдена".to_string());
        }
        if dir == self.root {
            return Err("Нельзя удалить корень хранилища".to_string());
        }
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        self.sync()
    }

    pub fn delete_deck(&mut self, deck_id: &str) -> Result<(), String> {
        let dir = self.deck_dir(deck_id)?;
        if !dir.join("deck.yaml").is_file() {
            return Err("Это папка, а не колода".to_string());
        }
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        self.sync()
    }

    pub fn move_deck(&mut self, deck_id: &str, target_group: &str) -> Result<String, String> {
        let source = self.deck_dir(deck_id)?;
        let source_rel = safe_relative(deck_id)?;
        let target_group = target_group.trim();
        let target_rel = if target_group.is_empty() {
            PathBuf::new()
        } else {
            safe_relative(target_group)?
        };
        if target_rel.starts_with(&source_rel) {
            return Err("Нельзя переместить колоду внутрь себя".to_string());
        }

        let source_parent = source_rel.parent().unwrap_or(Path::new(""));
        if source_parent == target_rel.as_path() {
            return Ok(deck_id.to_string());
        }

        let target_dir = if target_group.is_empty() {
            self.root.clone()
        } else {
            safe_join(&self.root, target_group)?
        };
        if !target_dir.is_dir() {
            return Err("Группа не найдена".to_string());
        }

        let folder = source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| "Не удалось определить имя папки колоды".to_string())?;
        let base = sanitize_segment(&folder);
        let mut destination = target_dir.join(&folder);
        let mut n = 2;
        while destination.exists() {
            destination = target_dir.join(format!("{}-{}", base, n));
            n += 1;
        }

        fs::rename(&source, &destination).map_err(|e| e.to_string())?;

        let new_id = destination
            .strip_prefix(&self.root)
            .unwrap_or(&destination)
            .to_string_lossy()
            .replace('\\', "/");
        let mut meta = read_deck_meta(&destination);
        meta.id = Some(new_id.clone());
        fs::write(
            destination.join("deck.yaml"),
            serde_yaml::to_string(&meta).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        self.sync()?;
        Ok(new_id)
    }

    pub fn export_deck(&self, deck_id: &str, dest: &Path) -> Result<(), String> {
        use std::io::Write;
        let dir = self.deck_dir(deck_id)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = fs::File::create(dest).map_err(|e| e.to_string())?;
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for entry in WalkDir::new(&dir).into_iter().filter_map(|item| item.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if file_name.starts_with('.')
                || file_name == "Thumbs.db"
                || file_name == "desktop.ini"
            {
                continue;
            }
            let rel = path
                .strip_prefix(&dir)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            writer
                .start_file(rel, options)
                .map_err(|e| e.to_string())?;
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            writer.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn import_deck(&mut self, source: &Path) -> Result<String, String> {
        use std::io::Read;
        if !source.is_file() {
            return Err("Файл не найден".to_string());
        }
        let file = fs::File::open(source).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|_| "Не удалось открыть архив .crdz".to_string())?;

        let meta: DeckMeta = {
            let mut entry = archive.by_name("deck.yaml").map_err(|_| {
                "В архиве нет deck.yaml — это не колода Cardz".to_string()
            })?;
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| e.to_string())?;
            serde_yaml::from_str(&text).unwrap_or_default()
        };

        let fallback = source
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "Импортированная колода".to_string());
        let display_name = meta
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(fallback);
        let base = sanitize_segment(&display_name);
        let mut folder = base.clone();
        let mut n = 2;
        while self.root.join(&folder).exists() {
            folder = format!("{}-{}", base, n);
            n += 1;
        }

        let target = self.root.join(&folder);
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;

        let extract = (|| -> Result<(), String> {
            let mut total: u64 = 0;
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
                let Some(relative) = entry.enclosed_name() else {
                    continue;
                };
                let relative = relative.to_path_buf();
                let relative_str = relative.to_string_lossy().replace('\\', "/");
                if relative_str.starts_with("__MACOSX") {
                    continue;
                }
                if entry.is_dir() {
                    fs::create_dir_all(target.join(&relative)).map_err(|e| e.to_string())?;
                    continue;
                }
                total = total.saturating_add(entry.size());
                if total > 4 * 1024 * 1024 * 1024 {
                    return Err("Архив слишком большой".to_string());
                }
                let out_path = target.join(&relative);
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();

        if let Err(err) = extract {
            let _ = fs::remove_dir_all(&target);
            return Err(err);
        }
        if !target.join("deck.yaml").is_file() {
            let _ = fs::remove_dir_all(&target);
            return Err("Архив не содержит deck.yaml".to_string());
        }
        for sub in ["notes", "media", "templates"] {
            let _ = fs::create_dir_all(target.join(sub));
        }
        regenerate_note_ids(&target)?;
        self.sync()?;
        Ok(folder)
    }

    pub fn import_media(&self, deck_id: &str, source: &Path) -> Result<String, String> {
        let deck_dir = self.deck_dir(deck_id)?;
        if !source.is_file() {
            return Err("Файл не найден".to_string());
        }
        let bytes = fs::read(source).map_err(|e| e.to_string())?;
        let hash = hex::encode(Sha256::digest(&bytes));
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        let ext: String = ext
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(8)
            .collect();
        let filename = if ext.is_empty() {
            hash[..12].to_string()
        } else {
            format!("{}.{}", &hash[..12], ext)
        };
        let media_dir = deck_dir.join("media");
        fs::create_dir_all(&media_dir).map_err(|e| e.to_string())?;
        let dest = media_dir.join(&filename);
        if !dest.exists() {
            fs::write(&dest, bytes).map_err(|e| e.to_string())?;
        }
        Ok(format!("media/{}", filename))
    }

    pub fn read_media(&self, deck_id: &str, rel: &str) -> Result<String, String> {
        let deck_dir = self.deck_dir(deck_id)?;
        let path = safe_join(&deck_dir, rel)?;
        if !path.is_file() {
            return Err("Медиафайл не найден".to_string());
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let mime = mime_for(&path);
        Ok(format!(
            "data:{};base64,{}",
            mime,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }

    pub fn scope_deck_ids(&self, scope: &ReviewScope) -> Result<Vec<String>, String> {
        let decks = discover_decks(&self.root)?;
        let ids: Vec<String> = decks.into_iter().map(|deck| deck.id).collect();
        match scope {
            ReviewScope::Deck { id } => {
                if !ids.iter().any(|deck_id| deck_id == id) {
                    return Err("Колода не найдена".to_string());
                }
                Ok(vec![id.clone()])
            }
            ReviewScope::Group { id } => {
                let dir = safe_join(&self.root, id)?;
                if !dir.is_dir() {
                    return Err("Папка не найдена".to_string());
                }
                let prefix = format!("{}/", id.trim_end_matches('/'));
                Ok(ids
                    .into_iter()
                    .filter(|deck_id| deck_id == id || deck_id.starts_with(&prefix))
                    .collect())
            }
            ReviewScope::All => Ok(ids),
        }
    }

    pub fn ai_material(
        &self,
        scope: &ReviewScope,
        max_chars: usize,
    ) -> Result<AiMaterial, String> {
        let deck_ids = self.scope_deck_ids(scope)?;
        if deck_ids.is_empty() {
            return Ok(AiMaterial {
                text: String::new(),
                total_notes: 0,
                included_notes: 0,
            });
        }

        let decks = discover_decks(&self.root)?;
        let mut deck_meta: HashMap<String, (String, String)> = HashMap::new();
        for deck in &decks {
            deck_meta.insert(
                deck.id.clone(),
                (deck.name.clone(), deck.description.clone()),
            );
        }

        let mut rows: Vec<(String, String, Vec<String>)> = Vec::new();
        for deck_id in &deck_ids {
            let mut stmt = self
                .conn
                .prepare("SELECT path, tags FROM notes WHERE deck_id = ?1 ORDER BY path")
                .map_err(|e| e.to_string())?;
            let mapped = stmt
                .query_map(params![deck_id], |row| {
                    let path: String = row.get(0)?;
                    let tags: String = row.get(1)?;
                    Ok((path, tags))
                })
                .map_err(|e| e.to_string())?;
            for row in mapped.flatten() {
                let tags: Vec<String> = serde_json::from_str(&row.1).unwrap_or_default();
                rows.push((deck_id.clone(), row.0, tags));
            }
        }

        let total_notes = rows.len();
        if total_notes == 0 {
            return Ok(AiMaterial {
                text: String::new(),
                total_notes: 0,
                included_notes: 0,
            });
        }

        let limit = 240.min(total_notes);
        let selected: Vec<(String, String, Vec<String>)> = if total_notes <= limit {
            rows
        } else {
            let step = total_notes as f64 / limit as f64;
            (0..limit)
                .filter_map(|index| rows.get((index as f64 * step) as usize).cloned())
                .collect()
        };

        let mut sections: HashMap<String, Vec<(String, String, Vec<String>)>> = HashMap::new();
        for (deck_id, rel, _tags) in selected {
            let Ok(path) = safe_join(&self.root, &rel) else {
                continue;
            };
            let Ok(note) = parse_note(&path) else {
                continue;
            };
            let (front, back) = note_question_answer(&note.fields);
            if front.trim().is_empty() {
                continue;
            }
            sections.entry(deck_id).or_default().push((
                clean_material_text(&front, 260),
                clean_material_text(&back, 520),
                note.tags,
            ));
        }

        let title = match scope {
            ReviewScope::Deck { id } => deck_meta
                .get(id)
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| "Колода".to_string()),
            ReviewScope::Group { id } => safe_join(&self.root, id)
                .ok()
                .map(|dir| read_group_meta(&dir))
                .and_then(|meta| meta.name)
                .unwrap_or_else(|| "Папка".to_string()),
            ReviewScope::All => "Всё хранилище".to_string(),
        };

        let mut text = String::new();
        text.push_str(&format!("Тема: {}\n", title));
        if total_notes > limit {
            text.push_str(&format!(
                "Это выборка {} из {} карточек.\n",
                limit, total_notes
            ));
        } else {
            text.push_str(&format!("Карточек в материале: {}.\n", total_notes));
        }

        let mut included = 0usize;
        'decks: for deck_id in &deck_ids {
            let Some(entries) = sections.get(deck_id) else {
                continue;
            };
            if text.chars().count() >= max_chars {
                break;
            }
            let (name, description) = deck_meta
                .get(deck_id)
                .cloned()
                .unwrap_or((prettify_label(deck_id.rsplit('/').next().unwrap_or(deck_id)), String::new()));
            text.push_str(&format!("\n### {}\n", name));
            if !description.trim().is_empty() {
                text.push_str(&format!(
                    "{}\n",
                    clean_material_text(&description, 220)
                ));
            }
            for (front, back, tags) in entries {
                let mut line = format!("- В: {}\n", front);
                if !back.trim().is_empty() {
                    line.push_str(&format!("  О: {}\n", back));
                }
                if !tags.is_empty() {
                    line.push_str(&format!("  Теги: {}\n", tags.join(", ")));
                }
                if text.chars().count() + line.chars().count() > max_chars {
                    break 'decks;
                }
                text.push_str(&line);
                included += 1;
            }
        }

        Ok(AiMaterial {
            text,
            total_notes,
            included_notes: included,
        })
    }

    pub fn next_card(
        &self,
        scope: &ReviewScope,
        include_future: bool,
        exclude: &[String],
    ) -> Result<Option<ReviewCard>, String> {
        let deck_ids = self.scope_deck_ids(scope)?;
        if deck_ids.is_empty() {
            return Ok(None);
        }
        let now = now_ms();

        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<Value> = Vec::new();

        let placeholders = vec!["?"; deck_ids.len()].join(", ");
        conditions.push(format!("deck_id IN ({})", placeholders));
        values.extend(deck_ids.into_iter().map(Value::Text));

        if !include_future {
            conditions.push("(state = 0 OR due IS NULL OR due <= ?)".to_string());
            values.push(Value::Integer(now));
        }
        if !exclude.is_empty() {
            let placeholders = vec!["?"; exclude.len()].join(", ");
            conditions.push(format!("card_id NOT IN ({})", placeholders));
            values.extend(exclude.iter().cloned().map(Value::Text));
        }

        let order = if include_future {
            "RANDOM()"
        } else {
            "(state = 0) ASC, COALESCE(due, 0) ASC"
        };
        let sql = format!(
            "SELECT card_id, note_id, deck_id, ord, state, stability, difficulty, last_review, due, lapses
             FROM cards WHERE {} ORDER BY {} LIMIT 1",
            conditions.join(" AND "),
            order
        );

        let row = self
            .conn
            .query_row(&sql, params_from_iter(values.iter()), |row| {
                Ok(CardRow {
                    card_id: row.get(0)?,
                    note_id: row.get(1)?,
                    deck_id: row.get(2)?,
                    ord: row.get(3)?,
                    state: row.get(4)?,
                    stability: row.get(5)?,
                    difficulty: row.get(6)?,
                    last_review: row.get(7)?,
                    due: row.get(8)?,
                    lapses: row.get(9)?,
                })
            })
            .optional()
            .map_err(|e| e.to_string())?;

        let Some(card) = row else {
            return Ok(None);
        };

        let rel: String = self
            .conn
            .query_row(
                "SELECT path FROM notes WHERE note_id = ?1",
                params![card.note_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Заметка карточки не найдена".to_string())?;
        let path = safe_join(&self.root, &rel)?;
        let note = parse_note(&path)?;
        let card_deck_id = card.deck_id.clone();
        let templates = self.load_templates(&card_deck_id)?;
        let template = templates
            .get(&note.note_type)
            .or_else(|| templates.get(DEFAULT_NOTE_TYPE))
            .cloned()
            .unwrap_or_else(builtin_template);
        let card_template = template
            .cards
            .get(card.ord.max(0) as usize)
            .or_else(|| template.cards.first())
            .cloned()
            .unwrap_or_else(|| builtin_template().cards[0].clone());

        let elapsed = elapsed_days(card.last_review, now);
        let memory = memory_state(card.stability, card.difficulty);
        let intervals = self.preview_intervals(memory, elapsed)?;

        Ok(Some(ReviewCard {
            card_id: card.card_id,
            note_id: card.note_id,
            deck_id: card_deck_id.clone(),
            deck_name: self.deck_name(&card_deck_id),
            deck_path: self.deck_dir(&card_deck_id)?.to_string_lossy().to_string(),
            fields: note.fields,
            tags: note.tags,
            card_name: card_template.name,
            front: card_template.front,
            back: card_template.back,
            state: card.state,
            intervals,
            is_new: card.state == 0,
        }))
    }

    pub fn answer_card(&mut self, card_id: &str, rating: u32) -> Result<AnswerResult, String> {
        let rating = rating.clamp(1, 4);
        let now = now_ms();
        let card = self
            .conn
            .query_row(
                "SELECT card_id, note_id, deck_id, ord, state, stability, difficulty, last_review, due, lapses
                 FROM cards WHERE card_id = ?1",
                params![card_id],
                |row| {
                    Ok(CardRow {
                        card_id: row.get(0)?,
                        note_id: row.get(1)?,
                        deck_id: row.get(2)?,
                        ord: row.get(3)?,
                        state: row.get(4)?,
                        stability: row.get(5)?,
                        difficulty: row.get(6)?,
                        last_review: row.get(7)?,
                        due: row.get(8)?,
                        lapses: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Карточка не найдена".to_string())?;

        let deck_id = card.deck_id.clone();

        let elapsed = elapsed_days(card.last_review, now);
        let memory = memory_state(card.stability, card.difficulty);
        let states = self
            .fsrs
            .next_states(memory, 0.9, elapsed)
            .map_err(|e| e.to_string())?;

        let (item, state, due, label) = if rating == 1 {
            (
                states.again,
                1,
                now + 10 * 60 * 1000,
                "10 мин".to_string(),
            )
        } else {
            let item = match rating {
                2 => states.hard,
                3 => states.good,
                _ => states.easy,
            };
            let days = item.interval.round().max(1.0) as i64;
            let label = format_days(item.interval);
            (
                item,
                2,
                now + days * 86_400_000,
                label,
            )
        };

        let lapses = card.lapses + if rating == 1 && card.state != 0 { 1 } else { 0 };

        self.conn
            .execute(
                "UPDATE cards SET state = ?1, due = ?2, stability = ?3, difficulty = ?4,
                    reps = reps + 1, lapses = ?5, last_review = ?6
                 WHERE card_id = ?7",
                params![
                    state,
                    due,
                    item.memory.stability as f64,
                    item.memory.difficulty as f64,
                    lapses,
                    now,
                    card.card_id
                ],
            )
            .map_err(|e| e.to_string())?;

        self.conn
            .execute(
                "INSERT INTO reviews
                    (card_id, deck_id, rating, reviewed_at, prev_state, next_state, prev_due, next_due,
                     stability, difficulty, elapsed_days)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    card.card_id,
                    deck_id,
                    rating as i64,
                    now,
                    card.state,
                    state,
                    card.due,
                    due,
                    item.memory.stability as f64,
                    item.memory.difficulty as f64,
                    elapsed as f64
                ],
            )
            .map_err(|e| e.to_string())?;

        Ok(AnswerResult {
            due,
            label,
            state,
        })
    }

    fn preview_intervals(
        &self,
        memory: Option<MemoryState>,
        elapsed: u32,
    ) -> Result<HashMap<String, String>, String> {
        let states = self
            .fsrs
            .next_states(memory, 0.9, elapsed)
            .map_err(|e| e.to_string())?;
        let mut out = HashMap::new();
        out.insert("again".to_string(), "10 мин".to_string());
        out.insert("hard".to_string(), format_days(states.hard.interval));
        out.insert("good".to_string(), format_days(states.good.interval));
        out.insert("easy".to_string(), format_days(states.easy.interval));
        Ok(out)
    }

    pub fn sync(&mut self) -> Result<(), String> {
        let decks = discover_decks(&self.root)?;
        let mut templates_by_deck: HashMap<String, HashMap<String, Template>> = HashMap::new();
        for deck in &decks {
            templates_by_deck.insert(deck.id.clone(), load_templates(&deck.dir));
        }

        let mut parsed: Vec<(String, ParsedNote)> = Vec::new();
        for deck in &decks {
            let notes_dir = deck.dir.join("notes");
            if !notes_dir.is_dir() {
                continue;
            }
            for entry in WalkDir::new(&notes_dir)
                .into_iter()
                .filter_map(|entry| entry.ok())
            {
                if !entry.file_type().is_file() || !is_card_file(entry.path()) {
                    continue;
                }
                let path = entry.path();
                match parse_note(path) {
                    Ok(mut note) => {
                        if note.id.trim().is_empty() {
                            note.id = ulid::Ulid::generate().to_string();
                            if let Err(err) = write_note_file(
                                path,
                                &note.id,
                                &note.note_type,
                                &note.tags,
                                &note.fields,
                                &template_fields(&templates_by_deck, &deck.id, &note.note_type),
                            ) {
                                eprintln!("cardz: cannot rewrite {}: {}", path.display(), err);
                            }
                        }
                        parsed.push((deck.id.clone(), note));
                    }
                    Err(err) => eprintln!("cardz: skipped {}: {}", path.display(), err),
                }
            }
        }

        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        let mut found_notes: HashSet<String> = HashSet::new();
        let mut found_cards: HashSet<String> = HashSet::new();

        for (deck_id, note) in &parsed {
            found_notes.insert(note.id.clone());
            let tags_json = serde_json::to_string(&note.tags).unwrap_or_else(|_| "[]".to_string());
            let preview = make_preview(&note.fields);
            let rel = relative_string(&self.root, &note.path);

            tx.execute(
                "INSERT INTO notes (note_id, deck_id, path, note_type, tags, preview, mtime)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(note_id) DO UPDATE SET
                    deck_id = excluded.deck_id,
                    path = excluded.path,
                    note_type = excluded.note_type,
                    tags = excluded.tags,
                    preview = excluded.preview,
                    mtime = excluded.mtime",
                params![
                    note.id,
                    deck_id,
                    rel,
                    note.note_type,
                    tags_json,
                    preview,
                    note.mtime
                ],
            )
            .map_err(|e| e.to_string())?;

            let card_count = templates_by_deck
                .get(deck_id)
                .and_then(|map| map.get(&note.note_type))
                .map(|template| template.cards.len())
                .unwrap_or(1)
                .max(1);

            for ord in 0..card_count {
                let card_id = format!("{}:{}", note.id, ord);
                found_cards.insert(card_id.clone());
                tx.execute(
                    "INSERT INTO cards (card_id, note_id, deck_id, ord)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(card_id) DO UPDATE SET
                        note_id = excluded.note_id,
                        deck_id = excluded.deck_id,
                        ord = excluded.ord",
                    params![card_id, note.id, deck_id, ord as i64],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        let stale_cards: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT card_id FROM cards")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|row| row.ok())
                .filter(|id| !found_cards.contains(id))
                .collect()
        };
        for id in stale_cards {
            tx.execute("DELETE FROM cards WHERE card_id = ?1", params![id])
                .map_err(|e| e.to_string())?;
        }

        let stale_notes: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT note_id FROM notes")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|row| row.ok())
                .filter(|id| !found_notes.contains(id))
                .collect()
        };
        for id in stale_notes {
            tx.execute("DELETE FROM notes WHERE note_id = ?1", params![id])
                .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn seed_demo(&mut self) -> Result<(), String> {
        if !discover_decks(&self.root)?.is_empty() {
            return Ok(());
        }
        let deck_id = self.create_deck(
            "Добро пожаловать",
            "Демо-колода. Удалите её, когда создадите свою.",
        )?;
        let deck_dir = self.deck_dir(&deck_id)?;
        let order = vec!["front".to_string(), "back".to_string()];

        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360" viewBox="0 0 640 360">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
<stop offset="0" stop-color="#8b7cff"/><stop offset="1" stop-color="#4ad6a4"/></linearGradient></defs>
<rect width="640" height="360" rx="28" fill="#11141c"/>
<rect x="150" y="70" width="300" height="200" rx="22" fill="url(#g)" opacity="0.25" transform="rotate(-8 300 170)"/>
<rect x="185" y="90" width="300" height="200" rx="22" fill="url(#g)"/>
<text x="335" y="205" text-anchor="middle" font-family="Segoe UI, sans-serif" font-size="52" font-weight="700" fill="#0d1017">cardz</text>
</svg>"##;
        let hash = hex::encode(Sha256::digest(svg.as_bytes()));
        fs::create_dir_all(deck_dir.join("media")).map_err(|e| e.to_string())?;
        let media_name = format!("{}.svg", &hash[..12]);
        fs::write(deck_dir.join("media").join(&media_name), svg).map_err(|e| e.to_string())?;

        let demo_notes: Vec<(&str, String, Vec<&str>)> = vec![
            (
                "Как хранятся карточки?",
                "Каждая заметка — обычный текстовый файл `.crd` (Markdown + YAML front matter) в папке `notes/`. Медиа лежит отдельно в `media/`. Всё это можно положить в git и переносить архивом.".to_string(),
                vec!["cardz", "формат"],
            ),
            (
                "¿Cómo se dice «бежать»?",
                "**correr** — to run\n\nЕщё: *caminar* — идти.".to_string(),
                vec!["испанский", "глаголы"],
            ),
            (
                "Столица Австралии",
                "**Канберра** — не Сидней и не Мельбурн. Город построили специально как столицу в 1913 году.".to_string(),
                vec!["география"],
            ),
            (
                "Что изображено на картинке?",
                format!(
                    "Логотип приложения **cardz**.\n\n![](media/{})",
                    media_name
                ),
                vec!["демо", "медиа"],
            ),
            (
                "Горячие клавиши в режиме повторения",
                "`Пробел` — показать ответ.\n\n`1` — Снова, `2` — Трудно, `3` — Хорошо, `4` — Легко.".to_string(),
                vec!["cardz", "шпаргалка"],
            ),
        ];

        for (front, back, tags) in demo_notes {
            let id = ulid::Ulid::generate().to_string();
            let mut fields = HashMap::new();
            fields.insert("front".to_string(), front.to_string());
            fields.insert("back".to_string(), back);
            let tag_list: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
            let notes_dir = deck_dir.join("notes");
            let path = unique_note_path(&notes_dir, &note_slug(front));
            write_note_file(&path, &id, "basic", &tag_list, &fields, &order)?;
        }
        self.sync()
    }
}

pub fn discover_decks(root: &Path) -> Result<Vec<DeckInfo>, String> {
    let mut out = Vec::new();
    walk_decks(root, root, 0, &mut out)?;
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

fn walk_decks(dir: &Path, root: &Path, depth: usize, out: &mut Vec<DeckInfo>) -> Result<(), String> {
    if depth > 12 {
        return Ok(());
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        if depth > 0 && matches!(name.as_str(), "notes" | "media" | "templates") {
            continue;
        }
        if path.join("deck.yaml").is_file() {
            let meta = read_deck_meta(&path);
            let rel = path.strip_prefix(root).unwrap_or(path.as_path());
            let id = rel.to_string_lossy().replace('\\', "/");
            out.push(DeckInfo {
                id,
                name: meta.name.unwrap_or_else(|| name.clone()),
                description: meta.description.unwrap_or_default(),
                dir: path.clone(),
            });
        }
        walk_decks(&path, root, depth + 1, out)?;
    }
    Ok(())
}

fn discover_groups(root: &Path, decks: &[DeckInfo]) -> Vec<GroupInfo> {
    let deck_ids: HashSet<String> = decks.iter().map(|deck| deck.id.clone()).collect();
    let mut map: HashMap<String, String> = HashMap::new();

    for deck in decks {
        let parts: Vec<&str> = deck.id.split('/').filter(|part| !part.is_empty()).collect();
        for index in 1..parts.len() {
            let id = parts[..index].join("/");
            if !deck_ids.contains(&id) {
                map.entry(id)
                    .or_insert_with(|| prettify_label(parts[index - 1]));
            }
        }
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            if matches!(name.as_str(), "notes" | "media" | "templates") {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            if path.join("deck.yaml").is_file() {
                stack.push(path);
                continue;
            }
            if path.join("group.yaml").is_file() {
                let meta = read_group_meta(&path);
                map.insert(
                    relative,
                    meta.name.unwrap_or_else(|| prettify_label(&name)),
                );
            }
            stack.push(path);
        }
    }

    let mut out: Vec<GroupInfo> = map
        .into_iter()
        .filter(|(id, _)| !deck_ids.contains(id))
        .map(|(id, name)| GroupInfo {
            dir: root.join(&id),
            id,
            name,
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn regenerate_note_ids(root: &Path) -> Result<(), String> {
    for entry in WalkDir::new(root).into_iter().filter_map(|item| item.ok()) {
        if !entry.file_type().is_file() || !is_card_file(entry.path()) {
            continue;
        }
        let path = entry.path();
        let parsed = parse_note(path).ok();
        let front = parsed
            .as_ref()
            .and_then(|note| note.fields.get("front").cloned())
            .or_else(|| {
                parsed
                    .as_ref()
                    .and_then(|note| note.fields.values().next().cloned())
            })
            .unwrap_or_default();
        let slug = note_slug(&front);
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let new_id = ulid::Ulid::generate().to_string();
        let (front_matter, body) = split_front_matter(&raw);
        let mut lines: Vec<String> = front_matter
            .map(|text| text.lines().map(|line| line.to_string()).collect())
            .unwrap_or_default();
        let mut replaced = false;
        for line in lines.iter_mut() {
            if line.trim_start().starts_with("id:") {
                *line = format!("id: {}", new_id);
                replaced = true;
                break;
            }
        }
        if !replaced {
            lines.insert(0, format!("id: {}", new_id));
        }
        let content = format!("---\n{}\n---\n{}", lines.join("\n"), body);
        let parent = path.parent().unwrap_or(root);
        let candidate = parent.join(format!("{}.crd", slug));
        let new_path = if paths_equal_ci(&candidate, path) {
            candidate
        } else {
            unique_note_path(parent, &slug)
        };
        fs::write(&new_path, content).map_err(|e| e.to_string())?;
        if !paths_equal_ci(&new_path, path) {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn prettify_label(segment: &str) -> String {
    let spaced = segment.replace(|ch| ch == '-' || ch == '_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn read_deck_meta(dir: &Path) -> DeckMeta {
    fs::read_to_string(dir.join("deck.yaml"))
        .ok()
        .and_then(|text| serde_yaml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn read_group_meta(dir: &Path) -> GroupMeta {
    fs::read_to_string(dir.join("group.yaml"))
        .ok()
        .and_then(|text| serde_yaml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn load_templates(deck_dir: &Path) -> HashMap<String, Template> {
    let mut map = HashMap::new();
    let dir = deck_dir.join("templates");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            match fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_yaml::from_str::<Template>(&text).ok())
            {
                Some(mut template) => {
                    normalize_template(&mut template);
                    map.insert(template.id.clone(), template);
                }
                None => eprintln!("cardz: cannot parse template {}", path.display()),
            }
        }
    }
    map.entry(DEFAULT_NOTE_TYPE.to_string())
        .or_insert_with(builtin_template);
    map
}

fn normalize_template(template: &mut Template) {
    template.fields = template
        .fields
        .iter()
        .map(|f| f.trim().to_lowercase())
        .filter(|f| !f.is_empty())
        .collect();
    if template.fields.is_empty() {
        template.fields = vec!["front".to_string(), "back".to_string()];
    }
    let labels: HashMap<String, String> = template
        .labels
        .iter()
        .map(|(k, v)| (k.trim().to_lowercase(), v.clone()))
        .collect();
    template.labels = labels;
    if template.cards.is_empty() {
        template.cards.push(CardTemplate {
            name: "Основная".to_string(),
            front: "{{front}}".to_string(),
            back: "{{back}}".to_string(),
        });
    }
}

pub fn builtin_template() -> Template {
    Template {
        id: DEFAULT_NOTE_TYPE.to_string(),
        name: "Базовая".to_string(),
        fields: vec!["front".to_string(), "back".to_string()],
        labels: HashMap::from([
            ("front".to_string(), "Вопрос".to_string()),
            ("back".to_string(), "Ответ".to_string()),
        ]),
        cards: vec![CardTemplate {
            name: "Вопрос → Ответ".to_string(),
            front: "{{front}}".to_string(),
            back: "{{back}}".to_string(),
        }],
    }
}

pub fn parse_note(path: &Path) -> Result<ParsedNote, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let raw = raw.replace("\r\n", "\n");
    let (front_matter, body) = split_front_matter(&raw);
    let meta: FrontMatter = match front_matter {
        Some(text) if !text.trim().is_empty() => {
            serde_yaml::from_str(text).map_err(|e| format!("некорректный front matter: {}", e))?
        }
        _ => FrontMatter::default(),
    };
    let note_type = if meta.note_type.trim().is_empty() {
        DEFAULT_NOTE_TYPE.to_string()
    } else {
        meta.note_type.trim().to_lowercase()
    };
    Ok(ParsedNote {
        id: meta.id.trim().to_string(),
        note_type,
        tags: meta.tags,
        fields: parse_fields(body),
        path: path.to_path_buf(),
        mtime: file_mtime(path),
    })
}

pub fn write_note_file(
    path: &Path,
    id: &str,
    note_type: &str,
    tags: &[String],
    fields: &HashMap<String, String>,
    order: &[String],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", id));
    out.push_str(&format!("type: {}\n", note_type));
    if tags.is_empty() {
        out.push_str("tags: []\n");
    } else {
        out.push_str(&format!(
            "tags: {}\n",
            serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
        ));
    }
    out.push_str("---\n\n");

    let mut written: HashSet<String> = HashSet::new();
    for name in order {
        if let Some(value) = fields.get(name) {
            out.push_str(&format!("## {}\n\n{}\n\n", name, value.trim()));
            written.insert(name.clone());
        }
    }
    let mut extras: Vec<&String> = fields
        .iter()
        .filter(|(k, v)| !written.contains(*k) && !v.trim().is_empty())
        .map(|(k, _)| k)
        .collect();
    extras.sort();
    for name in extras {
        out.push_str(&format!("## {}\n\n{}\n\n", name, fields[name].trim()));
    }

    fs::write(path, format!("{}\n", out.trim_end())).map_err(|e| e.to_string())
}

fn split_front_matter(raw: &str) -> (Option<&str>, &str) {
    if !raw.starts_with("---\n") {
        return (None, raw);
    }
    let rest = &raw[4..];
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(|c| c == '\n' || c == '\r');
        if trimmed == "---" || trimmed == "..." {
            let body_start = 4 + offset + line.len();
            return (Some(&rest[..offset]), &raw[body_start.min(raw.len())..]);
        }
        offset += line.len();
    }
    (None, raw)
}

fn parse_fields(body: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let mut current: Option<String> = None;
    let mut buf = String::new();
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some(name) = current.take() {
                fields.insert(name, buf.trim().to_string());
                buf.clear();
            }
            let name = rest.trim().to_lowercase();
            if !name.is_empty() {
                current = Some(name);
            }
        } else if current.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    if let Some(name) = current {
        fields.insert(name, buf.trim().to_string());
    }
    if fields.is_empty() {
        fields.insert("front".to_string(), body.trim().to_string());
    }
    fields
}

fn template_fields(
    templates: &HashMap<String, HashMap<String, Template>>,
    deck_id: &str,
    note_type: &str,
) -> Vec<String> {
    templates
        .get(deck_id)
        .and_then(|map| map.get(note_type))
        .map(|template| template.fields.clone())
        .unwrap_or_else(|| vec!["front".to_string(), "back".to_string()])
}

fn note_question_answer(fields: &HashMap<String, String>) -> (String, String) {
    let front_keys = ["front", "question", "term", "prompt"];
    let back_keys = ["back", "answer", "definition", "translation", "meaning"];
    let front_key = front_keys.iter().find(|key| fields.contains_key(**key)).copied();
    let front = front_key
        .and_then(|key| fields.get(key))
        .cloned()
        .unwrap_or_default();
    let back = back_keys
        .iter()
        .find_map(|key| fields.get(*key))
        .cloned()
        .unwrap_or_else(|| {
            fields
                .iter()
                .filter(|(key, _)| Some(key.as_str()) != front_key)
                .map(|(_, value)| value.clone())
                .collect::<Vec<_>>()
                .join(" ")
        });
    (front, back)
}

fn clean_material_text(raw: &str, max: usize) -> String {
    let mut pieces = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("![")
            || trimmed.contains("](media/")
            || trimmed.starts_with("<img")
        {
            continue;
        }
        pieces.push(trimmed);
    }
    let mut cleaned = String::new();
    let mut in_tag = false;
    for ch in pieces.join(" ").chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => cleaned.push(ch),
            _ => {}
        }
    }
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > max {
        format!("{}…", collapsed.chars().take(max).collect::<String>())
    } else {
        collapsed
    }
}

fn make_preview(fields: &HashMap<String, String>) -> String {
    let text = fields
        .get("front")
        .or_else(|| fields.values().next())
        .cloned()
        .unwrap_or_default();
    let flat: Vec<&str> = text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with("!["))
        .collect();
    let mut preview = flat.join(" ");
    for ch in ['#', '*', '`', '>', '_', '[', ']', '(', ')'] {
        preview = preview.replace(ch, "");
    }
    preview = preview.split_whitespace().collect::<Vec<_>>().join(" ");
    if preview.chars().count() > 140 {
        preview = preview.chars().take(140).collect::<String>() + "…";
    }
    preview
}

fn is_card_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("crd"))
        .unwrap_or(false)
}

fn file_mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn safe_relative(rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().replace('\\', "/");
    let path = Path::new(&rel);
    if path.is_absolute() {
        return Err("Абсолютные пути запрещены".to_string());
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err("Недопустимый путь".to_string()),
        }
    }
    if out.as_os_str().is_empty() {
        return Err("Пустой путь".to_string());
    }
    Ok(out)
}

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    Ok(root.join(safe_relative(rel)?))
}

const RESERVED_WINDOWS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn note_slug(front: &str) -> String {
    let text = strip_markdown(front);
    let mut out = String::new();
    let mut last_dash = false;
    for ch in text.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let mut slug: String = out.trim_matches('-').chars().take(60).collect();
    slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        return "note".to_string();
    }
    if RESERVED_WINDOWS_NAMES
        .iter()
        .any(|reserved| slug.eq_ignore_ascii_case(reserved))
    {
        return format!("note-{}", slug);
    }
    slug
}

fn strip_markdown(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '!' && index + 1 < chars.len() && chars[index + 1] == '[' {
            let mut end = index + 2;
            let start = end;
            while end < chars.len() && chars[end] != ']' {
                end += 1;
            }
            out.extend(chars[start..end].iter().copied());
            if end < chars.len() {
                end += 1;
            }
            if end < chars.len() && chars[end] == '(' {
                while end < chars.len() && chars[end] != ')' {
                    end += 1;
                }
                if end < chars.len() {
                    end += 1;
                }
            }
            index = end;
            continue;
        }
        if chars[index] == '[' {
            let mut end = index + 1;
            let start = end;
            while end < chars.len() && chars[end] != ']' {
                end += 1;
            }
            out.extend(chars[start..end].iter().copied());
            if end < chars.len() {
                end += 1;
            }
            if end < chars.len() && chars[end] == '(' {
                while end < chars.len() && chars[end] != ')' {
                    end += 1;
                }
                if end < chars.len() {
                    end += 1;
                }
            }
            index = end;
            continue;
        }
        if chars[index] == '<' {
            let mut end = index + 1;
            while end < chars.len() && chars[end] != '>' {
                end += 1;
            }
            if end < chars.len() {
                index = end + 1;
                continue;
            }
        }
        if matches!(chars[index], '#' | '*' | '_' | '`' | '~' | '>') {
            index += 1;
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

fn unique_note_path(notes_dir: &Path, slug: &str) -> PathBuf {
    let mut number = 1;
    loop {
        let name = if number == 1 {
            format!("{}.crd", slug)
        } else {
            format!("{}-{}.crd", slug, number)
        };
        if !file_name_taken(notes_dir, &name) {
            return notes_dir.join(name);
        }
        number += 1;
    }
}

fn file_name_taken(dir: &Path, name: &str) -> bool {
    let target = name.to_lowercase();
    fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry.file_name().to_string_lossy().to_lowercase() == target
            })
        })
        .unwrap_or(false)
}

fn paths_equal_ci(first: &Path, second: &Path) -> bool {
    first.to_string_lossy().to_lowercase() == second.to_string_lossy().to_lowercase()
}

fn sanitize_segment(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        if ch.is_alphanumeric()
            || matches!(
                ch,
                ' ' | '-' | '_' | '#' | '+' | '.' | '(' | ')' | '[' | ']' | '&'
            )
        {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim().trim_matches('.').trim();
    let mut result: String = out.chars().take(64).collect();
    result = result.trim_end().trim_end_matches('.').to_string();
    if result.is_empty() {
        result = "deck".to_string();
    }
    if RESERVED_WINDOWS_NAMES
        .iter()
        .any(|reserved| result.eq_ignore_ascii_case(reserved))
    {
        result.push('_');
    }
    result
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn memory_state(stability: Option<f64>, difficulty: Option<f64>) -> Option<MemoryState> {
    match (stability, difficulty) {
        (Some(stability), Some(difficulty)) if stability > 0.0 && difficulty > 0.0 => {
            Some(MemoryState {
                stability: stability as f32,
                difficulty: difficulty as f32,
            })
        }
        _ => None,
    }
}

fn elapsed_days(last_review: Option<i64>, now: i64) -> u32 {
    match last_review {
        Some(last) if last > 0 => (((now - last).max(0) as f64) / 86_400_000.0).floor() as u32,
        _ => 0,
    }
}

fn format_days(days: f32) -> String {
    if days < 1.0 {
        let minutes = (days * 24.0 * 60.0).round().max(1.0) as i64;
        if minutes < 60 {
            format!("{} мин", minutes)
        } else {
            format!("{} ч", (minutes as f64 / 60.0).round().max(1.0) as i64)
        }
    } else if days < 30.0 {
        format!("{} дн", days.round().max(1.0) as i64)
    } else if days < 365.0 {
        format!("{} мес", (days / 30.0).round().max(1.0) as i64)
    } else {
        format!("{} г", (days / 365.0).round().max(1.0) as i64)
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

const BASIC_TEMPLATE_YAML: &str = r#"id: basic
name: Базовая
fields:
  - front
  - back
labels:
  front: Вопрос
  back: Ответ
cards:
  - name: Вопрос → Ответ
    front: "{{front}}"
    back: |
      {{back}}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cardz-test-{}-{}", name, stamp));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn export_import_roundtrip() {
        let root = temp_dir("roundtrip");
        let mut vault = Vault::open(root.clone()).unwrap();
        let group = vault.create_group("Программирование/Базы данных").unwrap();
        assert_eq!(group, "Программирование/Базы данных");

        let deck_id = vault
            .create_deck("Программирование/Базы данных/C#", "demo")
            .unwrap();
        let mut fields = HashMap::new();
        fields.insert("front".to_string(), "Вопрос".to_string());
        fields.insert("back".to_string(), "Ответ".to_string());
        let note = vault
            .save_note(SaveNoteInput {
                deck_id: deck_id.clone(),
                note_id: None,
                note_type: "basic".to_string(),
                tags: vec!["тест".to_string()],
                fields,
            })
            .unwrap();

        let info = vault.info().unwrap();
        assert!(info
            .groups
            .iter()
            .any(|group| group.id == "Программирование/Базы данных"));
        assert!(info.decks.iter().any(|deck| deck.id == deck_id));

        let archive = root.join("export.crdz");
        vault.export_deck(&deck_id, &archive).unwrap();
        assert!(archive.is_file());

        let imported_id = vault.import_deck(&archive).unwrap();
        assert_ne!(imported_id, deck_id);
        assert_eq!(vault.list_notes(&deck_id, "").unwrap().len(), 1);

        let imported_notes = vault.list_notes(&imported_id, "").unwrap();
        assert_eq!(imported_notes.len(), 1);
        assert_ne!(imported_notes[0].id, note.id);
        let imported = vault.get_note(&imported_id, &imported_notes[0].id).unwrap();
        assert_eq!(imported.fields.get("back").map(String::as_str), Some("Ответ"));

        vault.create_group("Перемещения").unwrap();
        let moved_id = vault.move_deck(&imported_id, "Перемещения").unwrap();
        assert_eq!(moved_id, "Перемещения/C#");
        let moved_notes = vault.list_notes(&moved_id, "").unwrap();
        assert_eq!(moved_notes.len(), 1);
        assert_eq!(
            vault
                .get_note(&moved_id, &moved_notes[0].id)
                .unwrap()
                .fields
                .get("front")
                .map(String::as_str),
            Some("Вопрос")
        );

        let root_id = vault.move_deck(&moved_id, "").unwrap();
        assert_eq!(root_id, "C#");

        drop(vault);
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod review_scope_tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cardz-scope-{}-{}", name, stamp));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn add_note(vault: &mut Vault, deck_id: &str, front: &str) {
        let mut fields = HashMap::new();
        fields.insert("front".to_string(), front.to_string());
        fields.insert("back".to_string(), "ответ".to_string());
        vault
            .save_note(SaveNoteInput {
                deck_id: deck_id.to_string(),
                note_id: None,
                note_type: "basic".to_string(),
                tags: vec![],
                fields,
            })
            .unwrap();
    }

    #[test]
    fn group_scope_and_practice_exclusion() {
        let root = temp_dir("group");
        let mut vault = Vault::open(root.clone()).unwrap();
        vault.create_group("Folder/Nested").unwrap();
        let deck_a = vault.create_deck("Folder/Nested/A", "").unwrap();
        let deck_b = vault.create_deck("Folder/B", "").unwrap();
        let deck_c = vault.create_deck("Outside", "").unwrap();
        add_note(&mut vault, &deck_a, "A");
        add_note(&mut vault, &deck_b, "B");
        add_note(&mut vault, &deck_c, "C");

        let group_scope = ReviewScope::Group {
            id: "Folder".to_string(),
        };
        let mut ids = vault.scope_deck_ids(&group_scope).unwrap();
        ids.sort();
        let mut expected = vec![deck_a.clone(), deck_b.clone()];
        expected.sort();
        assert_eq!(ids, expected);

        let mut seen = Vec::new();
        while let Some(card) = vault.next_card(&group_scope, true, &seen).unwrap() {
            assert!(card.deck_id.starts_with("Folder/"));
            seen.push(card.card_id);
        }
        assert_eq!(seen.len(), 2);

        let all_scope = ReviewScope::All;
        let mut seen_all = Vec::new();
        while let Some(card) = vault.next_card(&all_scope, true, &seen_all).unwrap() {
            assert!(!seen_all.contains(&card.card_id));
            seen_all.push(card.card_id);
        }
        assert_eq!(seen_all.len(), 3);

        assert!(vault
            .next_card(&all_scope, false, &[])
            .unwrap()
            .is_some());

        vault.delete_deck(&deck_c).unwrap();
        assert!(!root.join(&deck_c).exists());
        assert!(!vault
            .info()
            .unwrap()
            .decks
            .iter()
            .any(|deck| deck.id == deck_c));
        assert!(vault.delete_deck("Folder").is_err());

        vault.delete_group("Folder").unwrap();
        assert!(!root.join("Folder").exists());
        let info = vault.info().unwrap();
        assert!(info.decks.is_empty());
        assert!(vault.info().unwrap().groups.is_empty());
        assert!(vault.delete_group("Folder").is_err());

        drop(vault);
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod note_name_tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cardz-name-{}-{}", name, stamp));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn slug_filenames_are_stable_and_unique() {
        assert_eq!(note_slug("¿Cómo se dice «бежать»?"), "cómo-se-dice-бежать");
        assert_eq!(note_slug("**Bold** [link](https://example.com)"), "bold-link");
        assert_eq!(note_slug(""), "note");
        assert_eq!(note_slug("CON"), "note-con");
        assert!(note_slug(&"x".repeat(200)).chars().count() <= 60);

        let root = temp_dir("slug");
        let mut vault = Vault::open(root.clone()).unwrap();
        let deck_id = vault.create_deck("Deck", "").unwrap();

        let mut fields = HashMap::new();
        fields.insert("front".to_string(), "Как хранятся карточки?".to_string());
        fields.insert("back".to_string(), "Ответ".to_string());
        let first = vault
            .save_note(SaveNoteInput {
                deck_id: deck_id.clone(),
                note_id: None,
                note_type: "basic".to_string(),
                tags: vec![],
                fields: fields.clone(),
            })
            .unwrap();
        assert!(
            first.path.ends_with("notes/как-хранятся-карточки.crd"),
            "{}",
            first.path
        );

        let second = vault
            .save_note(SaveNoteInput {
                deck_id: deck_id.clone(),
                note_id: None,
                note_type: "basic".to_string(),
                tags: vec![],
                fields: fields.clone(),
            })
            .unwrap();
        assert!(
            second.path.ends_with("notes/как-хранятся-карточки-2.crd"),
            "{}",
            second.path
        );

        let mut changed = HashMap::new();
        changed.insert("front".to_string(), "Новый вопрос".to_string());
        changed.insert("back".to_string(), "Ответ".to_string());
        let edited = vault
            .save_note(SaveNoteInput {
                deck_id,
                note_id: Some(first.id.clone()),
                note_type: "basic".to_string(),
                tags: vec![],
                fields: changed,
            })
            .unwrap();
        assert_eq!(edited.path, first.path);
        assert!(root.join(&first.path).is_file());

        drop(vault);
        let _ = fs::remove_dir_all(root);
    }
}
