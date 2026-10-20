import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  Loader2,
  Play,
  Plus,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { api } from "../lib/api";
import type {
  AiCardProposal,
  CommitResult,
  StartReview,
  VaultInfo,
} from "../lib/types";

type EditableCard = AiCardProposal & {
  enabled: boolean;
  tagsText: string;
};

function toEditable(cards: AiCardProposal[]): EditableCard[] {
  return cards.map((card) => ({
    ...card,
    enabled: !card.duplicate,
    tagsText: card.tags.join(", "),
  }));
}

function deckTitle(vault: VaultInfo, id: string): string {
  return vault.decks.find((deck) => deck.id === id)?.name ?? id;
}

export function AssistantPanel({
  vault,
  initialDeckId,
  onClose,
  onChanged,
  onStartReview,
}: {
  vault: VaultInfo;
  initialDeckId: string | null;
  onClose: () => void;
  onChanged: () => void;
  onStartReview: StartReview;
}) {
  const [deckId, setDeckId] = useState(
    initialDeckId && vault.decks.some((deck) => deck.id === initialDeckId)
      ? initialDeckId
      : vault.decks[0]?.id ?? "",
  );
  const [request, setRequest] = useState("");
  const [count, setCount] = useState(6);
  const [cards, setCards] = useState<EditableCard[]>([]);
  const [requests, setRequests] = useState<string[]>([]);
  const [generating, setGenerating] = useState(false);
  const [committing, setCommitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<CommitResult | null>(null);

  const selected = useMemo(
    () =>
      cards.filter(
        (card) => card.enabled && card.front.trim() && card.back.trim(),
      ),
    [cards],
  );
  const duplicates = cards.filter((card) => card.duplicate).length;

  useEffect(() => {
    setCreated(null);
  }, [deckId]);

  const generate = async () => {
    const text = request.trim();
    if (!text || !deckId || generating) return;
    setGenerating(true);
    setError(null);
    try {
      const proposals = await api.aiAssistantGenerate(
        deckId,
        text,
        cards.map((card) => card.front),
        count,
      );
      setCards((prev) => [...prev, ...toEditable(proposals)]);
      setRequests((prev) => [...prev, text]);
      setRequest("");
    } catch (err) {
      setError(String(err));
    } finally {
      setGenerating(false);
    }
  };

  const commit = async () => {
    if (!deckId || committing || selected.length === 0) return;
    setCommitting(true);
    setError(null);
    try {
      const result = await api.aiAssistantCommit(
        deckId,
        selected.map((card) => ({
          front: card.front.trim(),
          back: card.back.trim(),
          tags: card.tagsText
            .split(",")
            .map((tag) => tag.trim())
            .filter(Boolean),
          topic: card.topic,
        })),
      );
      setCreated(result);
      onChanged();
    } catch (err) {
      setError(String(err));
    } finally {
      setCommitting(false);
    }
  };

  const update = (index: number, patch: Partial<EditableCard>) => {
    setCards((prev) =>
      prev.map((card, cardIndex) =>
        cardIndex === index ? { ...card, ...patch } : card,
      ),
    );
  };

  return (
    <div
      className="modal-backdrop assistant-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !generating && !committing) {
          onClose();
        }
      }}
    >
      <div className="modal assistant-modal">
        <header className="editor-head">
          <div>
            <div className="eyebrow">Cardz AI</div>
            <h2>ИИ-ассистент</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Закрыть">
            <X size={18} />
          </button>
        </header>

        <div className="assistant-body">
          {vault.decks.length === 0 ? (
            <div className="empty-state settings-empty">
              <div className="empty-icon">
                <Sparkles size={22} />
              </div>
              <h3>Сначала создайте колоду</h3>
              <p>
                ИИ-ассистенту нужна колода, в которую он сможет добавить
                карточки.
              </p>
              <button className="ghost-btn" onClick={onClose}>
                Понятно
              </button>
            </div>
          ) : created ? (
            <div className="cards-success">
              <div className="cards-success-icon">
                <Check size={22} />
              </div>
              <h3>Карточки добавлены</h3>
              <p>
                Добавлено: <strong>{created.created}</strong>
                {created.skipped > 0 &&
                  `, пропущено дублей: ${created.skipped}`}
                . Колода: <strong>{deckTitle(vault, created.deckId)}</strong>
              </p>
              <div className="cards-success-actions">
                {created.created > 0 && (
                  <button
                    className="primary-btn"
                    onClick={() => {
                      onStartReview(
                        { kind: "deck", id: created.deckId },
                        deckTitle(vault, created.deckId),
                        "scheduled",
                        created.created,
                      );
                      onClose();
                    }}
                  >
                    <Play size={16} /> Повторить новые карточки
                  </button>
                )}
                <button
                  className="ghost-btn"
                  onClick={() => {
                    setCreated(null);
                    setCards([]);
                    setRequests([]);
                  }}
                >
                  <Plus size={16} /> Ещё карточки
                </button>
                <button className="ghost-btn" onClick={onClose}>
                  Закрыть
                </button>
              </div>
            </div>
          ) : (
            <>
              <p className="settings-hint">
                Опишите тему или попросите добавить карточки — ассистент
                предложит формулировки на своё усмотрение. Их можно
                отредактировать перед добавлением.
              </p>

              <div className="assistant-form">
                <div className="settings-grid">
                  <label className="field">
                    <span>Колода</span>
                    <select
                      value={deckId}
                      onChange={(event) => {
                        setDeckId(event.target.value);
                        setCards([]);
                        setRequests([]);
                        setError(null);
                      }}
                    >
                      {vault.decks.map((deck) => (
                        <option value={deck.id} key={deck.id}>
                          {deck.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>Сколько карточек</span>
                    <select
                      value={count}
                      onChange={(event) => setCount(Number(event.target.value))}
                    >
                      <option value={3}>3</option>
                      <option value={5}>5</option>
                      <option value={6}>6</option>
                      <option value={8}>8</option>
                      <option value={12}>12</option>
                    </select>
                  </label>
                </div>

                <label className="field">
                  <span>Запрос</span>
                  <textarea
                    value={request}
                    onChange={(event) => setRequest(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                        event.preventDefault();
                        void generate();
                      }
                    }}
                    rows={3}
                    placeholder="Например: «Сделай карточки про перцентили p95 и p99 — что это и как работает»"
                    disabled={generating}
                  />
                </label>

                {requests.length > 0 && (
                  <div className="assistant-requests">
                    {requests.map((item, index) => (
                      <span key={`${item}-${index}`} className="verdict-chip">
                        {item}
                      </span>
                    ))}
                  </div>
                )}

                {error && <div className="inline-error">{error}</div>}

                <div className="assistant-form-actions">
                  <span className="settings-hint">
                    Ctrl+Enter — предложить карточки
                  </span>
                  <button
                    className="primary-btn"
                    onClick={() => void generate()}
                    disabled={generating || !request.trim()}
                  >
                    {generating ? (
                      <Loader2 size={16} className="spin" />
                    ) : (
                      <Sparkles size={16} />
                    )}
                    {cards.length > 0 ? "Предложить ещё" : "Предложить карточки"}
                  </button>
                </div>
              </div>

              {cards.length > 0 && (
                <>
                  <div className="assistant-list-head">
                    <div>
                      <h3>Предложенные карточки</h3>
                      <span className="settings-hint">
                        Выбрано: {selected.length} из {cards.length}
                        {duplicates > 0 && ` · дублей: ${duplicates}`}
                      </span>
                    </div>
                    <button
                      className="ghost-btn small"
                      onClick={() => setCards([])}
                      disabled={generating || committing}
                    >
                      <Trash2 size={14} /> Очистить
                    </button>
                  </div>

                  <div className="cards-list">
                    {cards.map((card, index) => (
                      <div
                        className={`card-editor ${card.enabled ? "" : "off"}`}
                        key={`${index}-${card.front}`}
                      >
                        <div className="card-editor-head">
                          <label className="card-toggle">
                            <input
                              type="checkbox"
                              checked={card.enabled}
                              onChange={(event) =>
                                update(index, {
                                  enabled: event.target.checked,
                                })
                              }
                            />
                            <span>Добавить</span>
                          </label>
                          <span className="card-topic">
                            {card.topic || "Карточка"}
                          </span>
                          {card.duplicate && (
                            <span className="card-duplicate">уже есть</span>
                          )}
                          <button
                            className="icon-btn"
                            title="Убрать карточку"
                            onClick={() => update(index, { enabled: false })}
                          >
                            <Trash2 size={14} />
                          </button>
                        </div>
                        <label className="field">
                          <span>Вопрос</span>
                          <textarea
                            value={card.front}
                            onChange={(event) =>
                              update(index, { front: event.target.value })
                            }
                            rows={2}
                          />
                        </label>
                        <label className="field">
                          <span>Ответ</span>
                          <textarea
                            value={card.back}
                            onChange={(event) =>
                              update(index, { back: event.target.value })
                            }
                            rows={3}
                          />
                        </label>
                        <label className="field">
                          <span>Теги (через запятую)</span>
                          <input
                            value={card.tagsText}
                            onChange={(event) =>
                              update(index, { tagsText: event.target.value })
                            }
                          />
                        </label>
                      </div>
                    ))}
                  </div>

                  <div className="assistant-commit">
                    {error && <div className="inline-error">{error}</div>}
                    <button
                      className="primary-btn"
                      onClick={() => void commit()}
                      disabled={committing || selected.length === 0}
                    >
                      {committing ? (
                        <Loader2 size={16} className="spin" />
                      ) : (
                        <Check size={16} />
                      )}
                      Добавить {selected.length} карточек в «
                      {deckTitle(vault, deckId)}»
                    </button>
                  </div>
                </>
              )}

              {generating && (
                <div className="assistant-thinking">
                  <Loader2 size={18} className="spin" /> Придумываю карточки…
                </div>
              )}

              {!generating && cards.length === 0 && (
                <div className="assistant-tip">
                  <AlertTriangle size={14} /> Ассистент учтёт существующие
                  карточки колоды и не будет повторять их.
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
