use crate::db;
use crate::model::*;
use base64::Engine;
use fsrs::{FSRS, MemoryState};
use rusqlite::{params, OptionalExtension};
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
        let name = self
            .root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Cardz".to_string());
        Ok(VaultInfo {
            root: self.root.to_string_lossy().to_string(),
            name,
            decks: out,
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

        let root_rel =
            existing_rel.unwrap_or_else(|| format!("{}/notes/{}.crd", input.deck_id, note_id));
        let path = safe_join(&self.root, &root_rel)?;

        write_note_file(
            &path,
            &note_id,
            &input.note_type,
            &input.tags,
            &fields,
            &template.fields,
        )?;
        self.sync()?;

        Ok(NoteDetail {
            id: note_id,
            deck_id: input.deck_id,
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
        let name = name.trim();
        if name.is_empty() {
            return Err("Название колоды не может быть пустым".to_string());
        }
        let slug = slugify(name);
        let mut folder = slug.clone();
        let mut n = 2;
        while self.root.join(&folder).exists() {
            folder = format!("{}-{}", slug, n);
            n += 1;
        }
        let dir = self.root.join(&folder);
        fs::create_dir_all(dir.join("notes")).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("media")).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("templates")).map_err(|e| e.to_string())?;

        let meta = DeckMeta {
            id: Some(folder.clone()),
            name: Some(name.to_string()),
            description: Some(description.trim().to_string()),
        };
        fs::write(
            dir.join("deck.yaml"),
            serde_yaml::to_string(&meta).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::write(dir.join("templates").join("basic.yaml"), BASIC_TEMPLATE_YAML)
            .map_err(|e| e.to_string())?;
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

    pub fn next_card(&self, deck_id: &str) -> Result<Option<ReviewCard>, String> {
        let now = now_ms();
        let row = self
            .conn
            .query_row(
                "SELECT card_id, note_id, ord, state, stability, difficulty, last_review, due, lapses
                 FROM cards
                 WHERE deck_id = ?1 AND (state = 0 OR due IS NULL OR due <= ?2)
                 ORDER BY (state = 0) ASC, COALESCE(due, 0) ASC
                 LIMIT 1",
                params![deck_id, now],
                |row| {
                    Ok(CardRow {
                        card_id: row.get(0)?,
                        note_id: row.get(1)?,
                        ord: row.get(2)?,
                        state: row.get(3)?,
                        stability: row.get(4)?,
                        difficulty: row.get(5)?,
                        last_review: row.get(6)?,
                        due: row.get(7)?,
                        lapses: row.get(8)?,
                    })
                },
            )
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
        let templates = self.load_templates(deck_id)?;
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
            deck_id: deck_id.to_string(),
            deck_name: self.deck_name(deck_id),
            deck_path: self.deck_dir(deck_id)?.to_string_lossy().to_string(),
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
                "SELECT card_id, note_id, ord, state, stability, difficulty, last_review, due, lapses
                 FROM cards WHERE card_id = ?1",
                params![card_id],
                |row| {
                    Ok(CardRow {
                        card_id: row.get(0)?,
                        note_id: row.get(1)?,
                        ord: row.get(2)?,
                        state: row.get(3)?,
                        stability: row.get(4)?,
                        difficulty: row.get(5)?,
                        last_review: row.get(6)?,
                        due: row.get(7)?,
                        lapses: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Карточка не найдена".to_string())?;

        let deck_id: String = self
            .conn
            .query_row(
                "SELECT deck_id FROM cards WHERE card_id = ?1",
                params![card.card_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

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
            write_note_file(
                &deck_dir.join("notes").join(format!("{}.crd", id)),
                &id,
                "basic",
                &tag_list,
                &fields,
                &order,
            )?;
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
    if depth > 6 {
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
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let id = rel.to_string_lossy().replace('\\', "/");
            out.push(DeckInfo {
                id,
                name: meta.name.unwrap_or_else(|| name.clone()),
                description: meta.description.unwrap_or_default(),
                dir: path.clone(),
            });
        } else {
            walk_decks(&path, root, depth + 1, out)?;
        }
    }
    Ok(())
}

pub fn read_deck_meta(dir: &Path) -> DeckMeta {
    fs::read_to_string(dir.join("deck.yaml"))
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
            back: "{{front}}\n\n---\n\n{{back}}".to_string(),
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
            back: "{{front}}\n\n---\n\n{{back}}".to_string(),
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

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
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
    Ok(root.join(out))
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    let out: String = out.chars().take(60).collect();
    if out.is_empty() {
        "deck".to_string()
    } else {
        out
    }
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
      {{front}}

      ---

      {{back}}
"#;
