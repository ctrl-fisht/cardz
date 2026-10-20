use crate::model::Counts;
use rusqlite::Connection;
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS notes (
            note_id   TEXT PRIMARY KEY,
            deck_id   TEXT NOT NULL,
            path      TEXT NOT NULL,
            note_type TEXT NOT NULL,
            tags      TEXT NOT NULL DEFAULT '[]',
            preview   TEXT NOT NULL DEFAULT '',
            mtime     INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_notes_deck ON notes(deck_id);
         CREATE TABLE IF NOT EXISTS cards (
            card_id     TEXT PRIMARY KEY,
            note_id     TEXT NOT NULL,
            deck_id     TEXT NOT NULL,
            ord         INTEGER NOT NULL,
            state       INTEGER NOT NULL DEFAULT 0,
            due         INTEGER,
            stability   REAL,
            difficulty  REAL,
            reps        INTEGER NOT NULL DEFAULT 0,
            lapses      INTEGER NOT NULL DEFAULT 0,
            last_review INTEGER
         );
         CREATE INDEX IF NOT EXISTS idx_cards_deck ON cards(deck_id, state, due);
         CREATE INDEX IF NOT EXISTS idx_cards_note ON cards(note_id);
         CREATE TABLE IF NOT EXISTS reviews (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            card_id     TEXT NOT NULL,
            deck_id     TEXT NOT NULL,
            rating      INTEGER NOT NULL,
            reviewed_at INTEGER NOT NULL,
            prev_state  INTEGER,
            next_state  INTEGER,
            prev_due    INTEGER,
            next_due    INTEGER,
            stability   REAL,
            difficulty  REAL,
            elapsed_days REAL
         );
         CREATE INDEX IF NOT EXISTS idx_reviews_deck ON reviews(deck_id, reviewed_at);",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

pub fn deck_counts(conn: &Connection, deck_id: &str, now: i64, day_start: i64) -> Result<Counts, String> {
    conn.query_row(
        "SELECT
            COALESCE(SUM(CASE WHEN state = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN state <> 0 AND due IS NOT NULL AND due <= ?1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN state IN (1, 3) AND due IS NOT NULL AND due <= ?1 THEN 1 ELSE 0 END), 0),
            COUNT(*),
            (SELECT COUNT(*) FROM reviews r WHERE r.deck_id = ?2 AND r.reviewed_at >= ?3)
         FROM cards WHERE deck_id = ?2",
        rusqlite::params![now, deck_id, day_start],
        |row| {
            Ok(Counts {
                new: row.get(0)?,
                due: row.get(1)?,
                learning: row.get(2)?,
                total: row.get(3)?,
                reviewed_today: row.get(4)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

pub fn day_start_ms() -> i64 {
    use chrono::{Local, TimeZone};
    let now = Local::now();
    let start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    Local
        .from_local_datetime(&start)
        .earliest()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| now.timestamp_millis())
}
