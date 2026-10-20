import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowRight,
  FilePlus2,
  FolderTree,
  Infinity as InfinityIcon,
  Loader2,
  Play,
  Plus,
  Search,
  Sparkles,
  Trash2,
} from "lucide-react";
import { api } from "../lib/api";
import type { Deck, NoteDetail, NoteSummary, StartInterview, StartReview } from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";
import { NoteEditor } from "./NoteEditor";

function formatDate(ms: number): string {
  if (!ms) return "";
  return new Date(ms).toLocaleDateString("ru-RU", {
    day: "numeric",
    month: "short",
  });
}

function prettySegment(segment: string): string {
  const spaced = segment.replace(/[-_]+/g, " ");
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

export function DeckView({
  deck,
  version,
  onStartReview,
  onStartInterview,
  onDeleteDeck,
  onRefresh,
}: {
  deck: Deck;
  version: number;
  onStartReview: StartReview;
  onStartInterview: StartInterview;
  onDeleteDeck: (deck: Deck) => Promise<void>;
  onRefresh: () => Promise<void>;
}) {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<NoteDetail | "new" | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const requestId = useRef(0);

  const load = useCallback(async () => {
    const id = ++requestId.current;
    try {
      const result = await api.listNotes(deck.id, query);
      if (id === requestId.current) {
        setNotes(result);
        setError(null);
      }
    } catch (err) {
      if (id === requestId.current) setError(String(err));
    } finally {
      if (id === requestId.current) setLoading(false);
    }
  }, [deck.id, query]);

  useEffect(() => {
    setLoading(true);
    const timer = window.setTimeout(() => void load(), query ? 160 : 0);
    return () => window.clearTimeout(timer);
  }, [load, query, version]);

  const openNote = async (noteId: string) => {
    setOpening(noteId);
    try {
      const detail = await api.getNote(deck.id, noteId);
      setEditor(detail);
    } catch (err) {
      setError(String(err));
    } finally {
      setOpening(null);
    }
  };

  const afterSave = async () => {
    setEditor(null);
    await Promise.all([load(), onRefresh()]);
  };

  const totalDue = deck.counts.due + deck.counts.new;
  const pathParts = deck.id.split("/").filter(Boolean);

  return (
    <main className="content">
      <div className="deck-page">
        <header className="deck-head">
          <div>
            <div className="eyebrow breadcrumb" title={deck.id}>
              <FolderTree size={13} />
              {pathParts.map((part, index) => (
                <span className="crumb" key={`${part}-${index}`}>
                  {index === pathParts.length - 1
                    ? deck.name
                    : prettySegment(part)}
                  {index < pathParts.length - 1 && (
                    <span className="crumb-sep">/</span>
                  )}
                </span>
              ))}
            </div>
            <h1>{deck.name}</h1>
            <p className="deck-desc">
              {deck.description || "Без описания — просто папка с файлами .crd"}
            </p>
          </div>
          <div className="head-actions">
            <button
              className="ghost-btn"
              disabled={deck.counts.total === 0}
              onClick={() =>
                onStartReview(
                  { kind: "deck", id: deck.id },
                  deck.name,
                  "all",
                  deck.counts.total,
                )
              }
            >
              <InfinityIcon size={16} /> Все карточки
              {deck.counts.total > 0 && (
                <span className="btn-count">{deck.counts.total}</span>
              )}
            </button>
            <button
              className="ghost-btn"
              disabled={deck.counts.total === 0}
              onClick={() =>
                onStartInterview({ kind: "deck", id: deck.id }, deck.name)
              }
              title={
                deck.counts.total === 0
                  ? "Сначала добавьте карточки"
                  : "Провести собеседование по колоде"
              }
            >
              <Sparkles size={16} /> Чат с AI
            </button>
            <button className="ghost-btn" onClick={() => setEditor("new")}>
              <FilePlus2 size={16} /> Добавить
            </button>
            <button
              className="primary-btn"
              onClick={() =>
                onStartReview(
                  { kind: "deck", id: deck.id },
                  deck.name,
                  "scheduled",
                  totalDue,
                )
              }
              disabled={totalDue === 0}
            >
              <Play size={16} />
              Повторять
              {totalDue > 0 && <span className="btn-badge">{totalDue}</span>}
            </button>
            <button
              className="icon-btn danger-icon"
              title="Удалить колоду"
              onClick={() => {
                setDeleteError(null);
                setConfirmDelete(true);
              }}
            >
              <Trash2 size={16} />
            </button>
          </div>
        </header>

        <section className="stats">
          <div className="stat">
            <span className="stat-label">Новые</span>
            <span className="stat-value">{deck.counts.new}</span>
            <span className="stat-hint">ещё не видели</span>
          </div>
          <div className="stat">
            <span className="stat-label">К повторению</span>
            <span className="stat-value accent">{deck.counts.due}</span>
            <span className="stat-hint">по расписанию</span>
          </div>
          <div className="stat">
            <span className="stat-label">Всего</span>
            <span className="stat-value">{deck.counts.total}</span>
            <span className="stat-hint">
              {deck.counts.reviewedToday > 0
                ? `${deck.counts.reviewedToday} ответов сегодня`
                : "карточек в колоде"}
            </span>
          </div>
        </section>

        <section className="notes-panel">
          <div className="notes-toolbar">
            <div className="search">
              <Search size={15} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Поиск по карточкам и тегам…"
              />
            </div>
            <span className="notes-count">
              {loading ? "…" : `${notes.length} заметок`}
            </span>
          </div>

          {error && <div className="inline-error">{error}</div>}

          {!error && notes.length === 0 && !loading && (
            <div className="empty-state">
              <div className="empty-icon">
                {query ? <Search size={22} /> : <Plus size={22} />}
              </div>
              <h3>
                {query ? "Ничего не найдено" : "Колода пока пустая"}
              </h3>
              <p>
                {query
                  ? "Попробуйте другой запрос или очистите поиск."
                  : "Добавьте первую заметку — она сохранится файлом в notes/."}
              </p>
              {!query && (
                <button className="primary-btn" onClick={() => setEditor("new")}>
                  <Plus size={16} /> Создать карточку
                </button>
              )}
            </div>
          )}

          <div className="note-list">
            {notes.map((note) => (
              <button
                key={note.id}
                className="note-row"
                onClick={() => void openNote(note.id)}
              >
                <div className="note-main">
                  <div className="note-preview">
                    {note.preview || "Пустая заметка"}
                  </div>
                  <div className="note-tags">
                    <span className="type-chip">{note.noteType}</span>
                    {note.tags.map((tag) => (
                      <span key={tag} className="tag">
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="note-meta">
                  {opening === note.id ? (
                    <Loader2 size={16} className="spin" />
                  ) : (
                    <>
                      <span>{formatDate(note.modified)}</span>
                      <ArrowRight size={15} />
                    </>
                  )}
                </div>
              </button>
            ))}
          </div>
        </section>
      </div>

      {editor !== null && (
        <NoteEditor
          deck={deck}
          note={editor === "new" ? null : editor}
          onClose={() => setEditor(null)}
          onSaved={afterSave}
        />
      )}

      {confirmDelete && (
        <ConfirmDialog
          title="Удалить колоду?"
          message={
            <>
              Папка <strong>{deck.name}</strong> со всеми заметками, медиа и
              вложенными колодами будет удалена безвозвратно.
            </>
          }
          busy={deleting}
          error={deleteError}
          onCancel={() => {
            if (!deleting) setConfirmDelete(false);
          }}
          onConfirm={async () => {
            if (deleting) return;
            setDeleting(true);
            setDeleteError(null);
            try {
              await onDeleteDeck(deck);
              setConfirmDelete(false);
            } catch (err) {
              setDeleteError(String(err));
            } finally {
              setDeleting(false);
            }
          }}
        />
      )}
    </main>
  );
}
