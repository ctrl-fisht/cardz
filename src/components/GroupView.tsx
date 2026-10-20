import { useState } from "react";
import {
  ArrowRight,
  FolderTree,
  Infinity as InfinityIcon,
  Play,
  Sparkles,
  Trash2,
} from "lucide-react";
import type {
  Counts,
  Deck,
  Group,
  StartInterview,
  StartReview,
} from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";

function prettySegment(segment: string): string {
  const spaced = segment.replace(/[-_]+/g, " ");
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

function pluralDecks(count: number): string {
  const mod10 = count % 10;
  const mod100 = count % 100;
  if (mod10 === 1 && mod100 !== 11) return "колода";
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return "колоды";
  return "колод";
}

function sumCounts(decks: Deck[]): Counts {
  return decks.reduce<Counts>(
    (total, deck) => ({
      new: total.new + deck.counts.new,
      due: total.due + deck.counts.due,
      learning: total.learning + deck.counts.learning,
      total: total.total + deck.counts.total,
      reviewedToday: total.reviewedToday + deck.counts.reviewedToday,
    }),
    {
      new: 0,
      due: 0,
      learning: 0,
      total: 0,
      reviewedToday: 0,
    },
  );
}

export function GroupView({
  group,
  decks,
  onSelectDeck,
  onStartReview,
  onStartInterview,
  onDeleteGroup,
}: {
  group: Group;
  decks: Deck[];
  onSelectDeck: (id: string) => void;
  onStartReview: StartReview;
  onStartInterview: StartInterview;
  onDeleteGroup: (groupId: string) => Promise<void>;
}) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const counts = sumCounts(decks);
  const available = counts.new + counts.due;
  const parts = group.id.split("/").filter(Boolean);

  return (
    <main className="content">
      <div className="deck-page">
        <header className="deck-head">
          <div>
            <div className="eyebrow breadcrumb" title={group.id}>
              <FolderTree size={13} />
              {parts.map((part, index) => (
                <span className="crumb" key={`${part}-${index}`}>
                  {index === parts.length - 1
                    ? group.name
                    : prettySegment(part)}
                  {index < parts.length - 1 && (
                    <span className="crumb-sep">/</span>
                  )}
                </span>
              ))}
            </div>
            <h1>{group.name}</h1>
            <p className="deck-desc">
              Папка · {decks.length} {pluralDecks(decks.length)}
            </p>
          </div>
          <div className="head-actions">
            <button
              className="ghost-btn"
              disabled={counts.total === 0}
              onClick={() =>
                onStartReview(
                  { kind: "group", id: group.id },
                  group.name,
                  "all",
                  counts.total,
                )
              }
            >
              <InfinityIcon size={16} /> Все карточки
              {counts.total > 0 && (
                <span className="btn-count">{counts.total}</span>
              )}
            </button>
            <button
              className="ghost-btn"
              disabled={counts.total === 0}
              onClick={() =>
                onStartInterview({ kind: "group", id: group.id }, group.name)
              }
              title={
                counts.total === 0
                  ? "Сначала добавьте карточки"
                  : "Провести собеседование по папке"
              }
            >
              <Sparkles size={16} /> Чат с AI
            </button>
            <button
              className="primary-btn"
              disabled={available === 0}
              onClick={() =>
                onStartReview(
                  { kind: "group", id: group.id },
                  group.name,
                  "scheduled",
                  available,
                )
              }
            >
              <Play size={16} />
              Повторить
              {available > 0 && <span className="btn-badge">{available}</span>}
            </button>
            <button
              className="icon-btn danger-icon"
              title="Удалить папку"
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
            <span className="stat-value">{counts.new}</span>
            <span className="stat-hint">ещё не видели</span>
          </div>
          <div className="stat">
            <span className="stat-label">К повторению</span>
            <span className="stat-value accent">{counts.due}</span>
            <span className="stat-hint">по расписанию</span>
          </div>
          <div className="stat">
            <span className="stat-label">Всего</span>
            <span className="stat-value">{counts.total}</span>
            <span className="stat-hint">
              {counts.reviewedToday > 0
                ? `${counts.reviewedToday} ответов сегодня`
                : "карточек в папке"}
            </span>
          </div>
        </section>

        <section className="notes-panel">
          <div className="notes-toolbar">
            <span className="group-section-title">Колоды в папке</span>
            <span className="notes-count">{decks.length}</span>
          </div>

          {decks.length === 0 ? (
            <div className="empty-state">
              <div className="empty-icon">
                <FolderTree size={22} />
              </div>
              <h3>В папке пока нет колод</h3>
              <p>
                Добавьте колоду через иконку «+» у папки в сайдбаре — она
                появится здесь.
              </p>
            </div>
          ) : (
            <div className="note-list">
              {decks.map((deck) => {
                const relative =
                  deck.id === group.id
                    ? "колода папки"
                    : deck.id.slice(group.id.length + 1);
                return (
                  <button
                    key={deck.id}
                    className="note-row"
                    onClick={() => onSelectDeck(deck.id)}
                  >
                    <div className="note-main">
                      <div className="group-deck-title">{deck.name}</div>
                      <div className="group-deck-meta">
                        <span className="type-chip">{relative}</span>
                        <span className="group-count-pill">
                          новые {deck.counts.new}
                        </span>
                        <span className="group-count-pill accent">
                          к повторению {deck.counts.due}
                        </span>
                        <span className="group-count-pill">
                          всего {deck.counts.total}
                        </span>
                      </div>
                    </div>
                    <div className="note-meta">
                      <span>{deck.counts.total} карт.</span>
                      <ArrowRight size={15} />
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </section>
      </div>

      {confirmDelete && (
        <ConfirmDialog
          title="Удалить папку?"
          message={
            <>
              Папка <strong>{group.name}</strong> и все вложенные колоды со
              всеми карточками и медиа будут удалены безвозвратно.
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
              await onDeleteGroup(group.id);
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
