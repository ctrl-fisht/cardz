import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FileCode2,
  ImagePlus,
  Loader2,
  Save,
  Trash2,
  X,
} from "lucide-react";
import { api } from "../lib/api";
import { Markdown } from "../lib/markdown";
import { rewriteImageWidth } from "../lib/media";
import { fieldLabel, renderTemplate } from "../lib/template";
import type { Deck, NoteDetail, Template } from "../lib/types";

export function NoteEditor({
  deck,
  note,
  onClose,
  onSaved,
}: {
  deck: Deck;
  note: NoteDetail | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [templates, setTemplates] = useState<Template[]>([]);
  const [typeId, setTypeId] = useState(note?.noteType ?? "basic");
  const [fields, setFields] = useState<Record<string, string>>(
    note?.fields ?? {},
  );
  const [tags, setTags] = useState(note?.tags.join(", ") ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeField, setActiveField] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const refs = useRef<Record<string, HTMLTextAreaElement | null>>({});

  useEffect(() => {
    let alive = true;
    api
      .listTemplates(deck.id)
      .then((list) => {
        if (!alive) return;
        setTemplates(list);
        const initial =
          list.find((template) => template.id === (note?.noteType ?? "basic")) ??
          list[0];
        if (initial) {
          setTypeId(initial.id);
          setFields((prev) => {
            const next = { ...prev };
            for (const field of initial.fields) {
              if (next[field] === undefined) next[field] = "";
            }
            return next;
          });
        }
      })
      .catch((err) => {
        if (alive) setError(String(err));
      });
    return () => {
      alive = false;
    };
  }, [deck.id, note?.noteType]);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (confirmDelete) {
        setConfirmDelete(false);
        return;
      }
      if (!busy) onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [busy, confirmDelete, onClose]);

  const template = templates.find((item) => item.id === typeId) ?? templates[0];
  const fieldKeys = template?.fields ?? ["front", "back"];

  const chooseType = (id: string) => {
    setTypeId(id);
    const next = templates.find((item) => item.id === id);
    if (!next) return;
    setFields((prev) => {
      const merged = { ...prev };
      for (const field of next.fields) {
        if (merged[field] === undefined) merged[field] = "";
      }
      return merged;
    });
  };

  const pickMedia = async () => {
    try {
      const chosen = await open({
        multiple: false,
        filters: [
          {
            name: "Медиа",
            extensions: [
              "png",
              "jpg",
              "jpeg",
              "gif",
              "webp",
              "svg",
              "mp3",
              "wav",
              "ogg",
              "mp4",
              "webm",
            ],
          },
        ],
      });
      if (typeof chosen !== "string") return;
      const rel = await api.importMedia(deck.id, chosen);
      const field = activeField ?? fieldKeys[0];
      const textarea = refs.current[field];
      const text = fields[field] ?? "";
      const start = textarea?.selectionStart ?? text.length;
      const end = textarea?.selectionEnd ?? text.length;
      const prefix =
        start > 0 && !text.slice(0, start).endsWith("\n") ? "\n\n" : "";
      const snippet = `![](${rel})`;
      const next = text.slice(0, start) + prefix + snippet + text.slice(end);
      setFields((prev) => ({ ...prev, [field]: next }));
      window.requestAnimationFrame(() => {
        const element = refs.current[field];
        element?.focus();
        const caret = start + prefix.length + snippet.length;
        element?.setSelectionRange(caret, caret);
      });
    } catch (err) {
      setError(String(err));
    }
  };

  const resizeMedia = (src: string, width: string) => {
    setFields((prev) => {
      const next: Record<string, string> = {};
      let changed = false;
      for (const [key, value] of Object.entries(prev)) {
        const updated = rewriteImageWidth(value, src, width);
        if (updated !== value) changed = true;
        next[key] = updated;
      }
      return changed ? next : prev;
    });
  };

  const save = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const tagList = tags
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean);
      await api.saveNote({
        deckId: deck.id,
        noteId: note?.id,
        noteType: typeId,
        tags: tagList,
        fields,
      });
      onSaved();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const performDelete = async () => {
    if (!note) return;
    setBusy(true);
    setError(null);
    try {
      await api.deleteNote(deck.id, note.id);
      onSaved();
    } catch (err) {
      setError(String(err));
      setBusy(false);
      setConfirmDelete(false);
    }
  };

  const previewCard = template?.cards[0];
  const previewFront = previewCard
    ? renderTemplate(previewCard.front, fields)
    : "";
  const previewBack = previewCard
    ? renderTemplate(previewCard.back, fields, previewFront)
    : "";

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div className="modal editor">
        <header className="editor-head">
          <div>
            <div className="eyebrow">Заметка</div>
            <h2>{note ? "Редактирование" : "Новая карточка"}</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Закрыть">
            <X size={18} />
          </button>
        </header>

        <div className="editor-body">
          <div className="editor-meta">
            <label className="field">
              <span>Тип заметки</span>
              <select
                value={typeId}
                onChange={(event) => chooseType(event.target.value)}
              >
                {templates.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field grow">
              <span>Теги</span>
              <input
                value={tags}
                onChange={(event) => setTags(event.target.value)}
                placeholder="через запятую: испанский, глаголы"
              />
            </label>
          </div>

          <div className="editor-toolbar">
            <button
              className="ghost-btn small"
              onClick={() => void pickMedia()}
              type="button"
            >
              <ImagePlus size={15} /> Медиа
            </button>
            <span className="editor-hint">
              Markdown · предпросмотр обновляется сразу
            </span>
          </div>

          <div className="editor-split">
            <div className="fields">
              {fieldKeys.map((field, index) => (
                <label className="field" key={field}>
                  <span>
                    {fieldLabel(template, field)}
                    <em>{field}</em>
                  </span>
                  <textarea
                    autoFocus={index === 0 && !note}
                    ref={(element) => {
                      refs.current[field] = element;
                    }}
                    rows={index === 0 ? 6 : 8}
                    value={fields[field] ?? ""}
                    onFocus={() => setActiveField(field)}
                    onChange={(event) =>
                      setFields((prev) => ({
                        ...prev,
                        [field]: event.target.value,
                      }))
                    }
                    placeholder={
                      index === 0
                        ? "Вопрос или термин…"
                        : "Ответ, перевод, пояснение…"
                    }
                  />
                </label>
              ))}
            </div>

            <aside className="live-preview">
              <div className="live-preview-head">
                <span>Предпросмотр</span>
                <span className="live-dot" />
              </div>
              <div className="preview-block">
                <span className="preview-label">Вопрос</span>
                <Markdown
                  text={previewFront}
                  deckId={deck.id}
                  editable
                  onResize={resizeMedia}
                />
              </div>
              <div className="preview-block answer">
                <span className="preview-label">Ответ</span>
                <Markdown
                  text={previewBack}
                  deckId={deck.id}
                  editable
                  onResize={resizeMedia}
                />
              </div>
            </aside>
          </div>

          {error && <div className="inline-error">{error}</div>}

          <div className="editor-footnote">
            <FileCode2 size={13} />
            Файл: {note ? `notes/${note.id}.crd` : "новый файл в notes/"} ·
            Markdown + YAML front matter
          </div>
        </div>

        <footer className="editor-foot">
          {note && (
            <button
              className="danger-btn"
              onClick={() => setConfirmDelete(true)}
              disabled={busy}
            >
              <Trash2 size={15} /> Удалить
            </button>
          )}
          <div className="spacer" />
          <button className="ghost-btn" onClick={onClose} disabled={busy}>
            Отмена
          </button>
          <button
            className="primary-btn"
            onClick={() => void save()}
            disabled={busy}
          >
            {busy ? <Loader2 size={16} className="spin" /> : <Save size={16} />}
            Сохранить
          </button>
        </footer>
      </div>

      {confirmDelete && (
        <div
          className="confirm-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setConfirmDelete(false);
          }}
        >
          <div className="confirm-dialog">
            <div className="confirm-icon">
              <Trash2 size={18} />
            </div>
            <h3>Удалить заметку?</h3>
            <p>
              Файл <code>notes/{note?.id}.crd</code> и прогресс по его
              карточкам будут удалены безвозвратно.
            </p>
            <div className="confirm-actions">
              <button
                className="ghost-btn"
                onClick={() => setConfirmDelete(false)}
                disabled={busy}
              >
                Отмена
              </button>
              <button
                className="danger-btn solid"
                onClick={() => void performDelete()}
                disabled={busy}
              >
                {busy ? (
                  <Loader2 size={15} className="spin" />
                ) : (
                  <Trash2 size={15} />
                )}
                Удалить
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
