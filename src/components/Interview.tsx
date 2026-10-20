import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  ArrowLeft,
  Check,
  Clock,
  Loader2,
  MessageSquare,
  Play,
  RefreshCw,
  Send,
  Sparkles,
  Square,
  ThumbsUp,
  Trash2,
  X,
} from "lucide-react";
import { api } from "../lib/api";
import { Markdown } from "../lib/markdown";
import type {
  AiCardProposal,
  AiEvent,
  AiMessage,
  AiVerdict,
  ReviewScope,
  StartReview,
  VaultInfo,
} from "../lib/types";

type Target =
  | { kind: "new"; scope: ReviewScope; title: string }
  | { kind: "history"; id: string };

type Stage = "loading" | "chat" | "error" | "verdict" | "cards";

type EditableCard = AiCardProposal & {
  enabled: boolean;
  tagsText: string;
};

function formatClock(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const minutes = Math.floor(seconds / 60);
  return `${String(minutes).padStart(2, "0")}:${String(seconds % 60).padStart(
    2,
    "0",
  )}`;
}

function normalizeKey(text: string): string {
  return text.toLocaleLowerCase("ru").replace(/[^\p{L}\p{N}]+/gu, "");
}

function levelLabel(level: AiVerdict["level"]): string {
  if (level === "weak") return "Есть заметные пробелы";
  if (level === "strong") return "Уверенный уровень";
  return "База есть, нужна практика";
}

function severityLabel(severity: string): string {
  if (severity === "high") return "важно";
  if (severity === "low") return "мелочь";
  return "средне";
}

function previewCards(cards: AiCardProposal[]): EditableCard[] {
  return cards.map((card) => ({
    ...card,
    enabled: true,
    tagsText: card.tags.join(", "),
  }));
}

function deckTitle(vault: VaultInfo, id: string): string {
  return vault.decks.find((deck) => deck.id === id)?.name ?? id;
}

export function Interview({
  target,
  vault,
  onClose,
  onOpenSettings,
  onChanged,
  onStartReview,
}: {
  target: Target;
  vault: VaultInfo;
  onClose: () => void;
  onOpenSettings: () => void;
  onChanged: () => void;
  onStartReview: StartReview;
}) {
  const [stage, setStage] = useState<Stage>("loading");
  const [readOnly, setReadOnly] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [interviewId, setInterviewId] = useState<string | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [scope, setScope] = useState<ReviewScope | null>(null);
  const [messages, setMessages] = useState<AiMessage[]>([]);
  const [verdict, setVerdict] = useState<AiVerdict | null>(null);
  const [cards, setCards] = useState<EditableCard[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [loadingCards, setLoadingCards] = useState(false);
  const [committing, setCommitting] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [objection, setObjection] = useState("");
  const [showObjection, setShowObjection] = useState(false);
  const [showTranscript, setShowTranscript] = useState(false);
  const [startedAt, setStartedAt] = useState(Date.now());
  const [now, setNow] = useState(Date.now());
  const [frozenSeconds, setFrozenSeconds] = useState<number | null>(null);
  const [materialTotal, setMaterialTotal] = useState(0);
  const [materialIncluded, setMaterialIncluded] = useState(0);
  const [destination, setDestination] = useState("");
  const [newDeckName, setNewDeckName] = useState("");
  const [createdInfo, setCreatedInfo] = useState<{
    created: number;
    skipped: number;
    deckId: string;
  } | null>(null);
  const [duplicates, setDuplicates] = useState<Set<string>>(new Set());

  const bottomRef = useRef<HTMLDivElement | null>(null);
  const activeSendRef = useRef(false);
  const assistantIdRef = useRef<string | null>(null);
  const textPartIdRef = useRef<string | null>(null);
  const streamRef = useRef("");
  const sawDeltaRef = useRef(false);

  const scopeDecks = useMemo(() => {
    if (!scope) return [];
    if (scope.kind === "deck") {
      return vault.decks.filter((deck) => deck.id === scope.id);
    }
    if (scope.kind === "group") {
      const prefix = `${scope.id}/`;
      return vault.decks.filter(
        (deck) => deck.id === scope.id || deck.id.startsWith(prefix),
      );
    }
    return vault.decks;
  }, [scope, vault.decks]);

  const defaultDestination = useMemo(() => {
    if (scope?.kind === "deck") return scope.id;
    if (scopeDecks.length === 1) return scopeDecks[0].id;
    return "__new__";
  }, [scope, scopeDecks]);

  useEffect(() => {
    setDestination(defaultDestination);
    if (scope?.kind === "group") {
      setNewDeckName(`AI-разбор: ${title}`);
    }
  }, [defaultDestination, scope, title]);

  const load = useCallback(async () => {
    setStage("loading");
    setError(null);
    try {
      if (target.kind === "history") {
        const result = await api.aiInterviewResume(target.id);
        setInterviewId(result.record.id);
        setSessionId(result.sessionId);
        setReadOnly(result.readOnly);
        setNotice(result.notice);
        setTitle(result.record.title);
        setScope(result.record.scope);
        setMessages(result.record.messages);
        setVerdict(result.record.verdict);
        setCards(previewCards(result.record.cards));
        setMaterialTotal(result.record.materialTotal);
        setMaterialIncluded(result.record.materialIncluded);
        setStartedAt(Date.now() - (result.record.durationSec || 0) * 1000);
        setFrozenSeconds(result.record.verdict ? result.record.durationSec : null);
        setStage(result.record.verdict ? "verdict" : "chat");
        return;
      }
      const result = await api.aiInterviewStart(target.scope, target.title);
      setInterviewId(result.interviewId);
      setSessionId(result.sessionId);
      setTitle(target.title);
      setScope(target.scope);
      setMessages([
        {
          role: "assistant",
          content: result.reply,
          at: Date.now(),
        },
      ]);
      setMaterialTotal(result.materialTotal);
      setMaterialIncluded(result.materialIncluded);
      setStartedAt(Date.now());
      setFrozenSeconds(null);
      setStage("chat");
    } catch (err) {
      setError(String(err));
      setStage("error");
    }
  }, [target]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (stage !== "chat" && stage !== "verdict") return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [stage]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, streamText, sending]);

  useEffect(() => {
    if (!sessionId) return;
    let alive = true;
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await listen<AiEvent>("ai://event", (event) => {
        if (!alive || !activeSendRef.current) return;
        const payload = event.payload;
        const properties = payload.properties as Record<string, unknown>;
        if (payload.type === "message.updated") {
          const info = properties.info as
            | {
                role?: string;
                sessionID?: string;
                error?: { data?: { message?: string } } | null;
              }
            | undefined;
          if (!info || info.sessionID !== sessionId) return;
          if (info.role === "assistant" && !assistantIdRef.current) {
            const id = (info as { id?: string }).id;
            if (id) assistantIdRef.current = id;
          }
          if (info.error) {
            const message = info.error.data?.message;
            setError(message || "Модель вернула ошибку");
          }
          return;
        }
        if (payload.type === "message.part.updated") {
          const part = properties.part as
            | {
                type?: string;
                text?: string;
                id?: string;
                messageID?: string;
                sessionID?: string;
              }
            | undefined;
          if (!part || part.sessionID !== sessionId) return;
          if (
            part.type === "text" &&
            part.messageID &&
            part.messageID === assistantIdRef.current
          ) {
            if (part.id) textPartIdRef.current = part.id;
            if (!sawDeltaRef.current) {
              streamRef.current = part.text ?? "";
              setStreamText(streamRef.current);
            }
          }
          return;
        }
        if (payload.type === "message.part.delta") {
          if (properties.sessionID !== sessionId) return;
          if (properties.field !== "text") return;
          if (properties.messageID !== assistantIdRef.current) return;
          if (properties.partID !== textPartIdRef.current) return;
          const delta = typeof properties.delta === "string" ? properties.delta : "";
          if (!delta) return;
          sawDeltaRef.current = true;
          streamRef.current += delta;
          setStreamText(streamRef.current);
        }
      });
      if (!alive) unlisten?.();
    })();
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [sessionId]);

  const elapsed =
    frozenSeconds ?? Math.max(0, Math.floor((now - startedAt) / 1000));

  const turnCount = messages.filter((message) => message.role === "user").length;

  const send = async (forced?: string) => {
    const text = (forced ?? input).trim();
    if (!text || !sessionId || sending || readOnly) return;
    if (!forced) setInput("");
    setError(null);
    setMessages((prev) => [
      ...prev,
      { role: "user", content: text, at: Date.now() },
    ]);
    setSending(true);
    setStreamText("");
    streamRef.current = "";
    sawDeltaRef.current = false;
    assistantIdRef.current = null;
    textPartIdRef.current = null;
    activeSendRef.current = true;
    try {
      const reply = await api.aiInterviewSend(sessionId, text);
      setMessages((prev) => [
        ...prev,
        { role: "assistant", content: reply, at: Date.now() },
      ]);
      onChanged();
    } catch (err) {
      setError(String(err));
    } finally {
      activeSendRef.current = false;
      setSending(false);
      setStreamText("");
      streamRef.current = "";
    }
  };

  const stop = async () => {
    if (!sessionId) return;
    await api.aiInterviewAbort(sessionId).catch(() => undefined);
  };

  const finish = async (withObjection = false) => {
    if (!sessionId || finishing) return;
    const objectionText = withObjection ? objection.trim() : "";
    if (withObjection && !objectionText) return;
    setFinishing(true);
    setError(null);
    setStreamText("");
    streamRef.current = "";
    sawDeltaRef.current = false;
    activeSendRef.current = false;
    if (objectionText) {
      setMessages((prev) => [
        ...prev,
        { role: "user", content: objectionText, at: Date.now() },
      ]);
    }
    try {
      const result = await api.aiInterviewVerdict(
        sessionId,
        objectionText || null,
      );
      if (result.reply) {
        setMessages((prev) => [
          ...prev,
          { role: "assistant", content: result.reply ?? "", at: Date.now() },
        ]);
      }
      setVerdict(result.verdict);
      setFrozenSeconds(Math.max(0, Math.floor((Date.now() - startedAt) / 1000)));
      setStage("verdict");
      setShowObjection(false);
      setObjection("");
      setShowTranscript(false);
      onChanged();
    } catch (err) {
      setError(String(err));
    } finally {
      setFinishing(false);
    }
  };

  const generateCards = async () => {
    if (!sessionId || loadingCards) return;
    setLoadingCards(true);
    setError(null);
    try {
      const proposals = await api.aiInterviewCards(sessionId);
      setCards(previewCards(proposals));
      setStage("cards");
      setCreatedInfo(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoadingCards(false);
    }
  };

  useEffect(() => {
    if (stage !== "cards") return;
    let alive = true;
    const deckId = destination === "__new__" ? null : destination;
    if (!deckId) {
      setDuplicates(new Set());
      return;
    }
    api
      .listNotes(deckId)
      .then((notes) => {
        if (!alive) return;
        setDuplicates(
          new Set(notes.map((note) => normalizeKey(note.preview)).filter(Boolean)),
        );
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [stage, destination]);

  const commit = async () => {
    if (!interviewId || committing) return;
    const selected = cards.filter((card) => card.enabled && card.front.trim() && card.back.trim());
    if (selected.length === 0) return;
    setCommitting(true);
    setError(null);
    try {
      let deckId = destination;
      if (deckId === "__new__") {
        const name = newDeckName.trim();
        if (!name) {
          setError("Введите название новой колоды");
          setCommitting(false);
          return;
        }
        const path =
          scope?.kind === "group" ? `${scope.id}/${name}` : name;
        const created = await api.createDeck(path);
        deckId = created.deckId;
      }
      const result = await api.aiInterviewCommit(
        interviewId,
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
      setCreatedInfo(result);
      setDestination(deckId);
      onChanged();
    } catch (err) {
      setError(String(err));
    } finally {
      setCommitting(false);
    }
  };

  const close = async () => {
    if (sending || finishing) {
      if (sessionId) await api.aiInterviewAbort(sessionId).catch(() => undefined);
    }
    onClose();
  };

  const updateCard = (index: number, patch: Partial<EditableCard>) => {
    setCards((prev) =>
      prev.map((card, cardIndex) =>
        cardIndex === index ? { ...card, ...patch } : card,
      ),
    );
  };

  const selectedCards = cards.filter(
    (card) => card.enabled && card.front.trim() && card.back.trim(),
  );

  return (
    <div className="interview">
      <header className="interview-head">
        <button className="icon-btn" onClick={() => void close()} title="Закрыть">
          <ArrowLeft size={18} />
        </button>
        <div className="interview-title">
          <div className="eyebrow">AI-собеседование</div>
          <h1>{title || "Подготовка…"}</h1>
          <div className="interview-meta">
            <span>
              <Clock size={13} /> {formatClock(elapsed)}
            </span>
            <span>Вопросов: {turnCount}</span>
            {materialTotal > 0 && (
              <span>
                Материал: {materialIncluded || materialTotal} из {materialTotal}
              </span>
            )}
            {notice && <span className="interview-notice">{notice}</span>}
          </div>
        </div>
        <div className="interview-actions">
          {stage === "chat" && !readOnly && (
            <button
              className="ghost-btn"
              onClick={() => void finish(false)}
              disabled={finishing || sending || turnCount < 1}
            >
              {finishing ? (
                <Loader2 size={16} className="spin" />
              ) : (
                <Check size={16} />
              )}
              Завершить
            </button>
          )}
          {stage === "verdict" && (
            <button
              className="ghost-btn"
              onClick={() => setShowTranscript((value) => !value)}
            >
              <MessageSquare size={16} />
              {showTranscript ? "Скрыть разговор" : "Показать разговор"}
            </button>
          )}
          <button className="icon-btn" onClick={() => void close()} title="Выйти">
            <X size={18} />
          </button>
        </div>
      </header>

      {stage === "loading" && (
        <div className="interview-loading">
          <div className="interview-orb">
            <Sparkles size={24} />
          </div>
          <h2>Собеседник готовится</h2>
          <p>Собираем материал и запускаем OpenCode…</p>
          <Loader2 size={20} className="spin" />
        </div>
      )}

      {stage === "error" && (
        <div className="interview-loading">
          <div className="interview-orb error">
            <AlertTriangle size={24} />
          </div>
          <h2>Не удалось начать собеседование</h2>
          <p className="interview-error-text">{error}</p>
          <div className="interview-error-actions">
            <button className="ghost-btn" onClick={onOpenSettings}>
              <Sparkles size={16} /> Настройки ИИ
            </button>
            <button className="primary-btn" onClick={() => void load()}>
              <RefreshCw size={16} /> Повторить
            </button>
          </div>
        </div>
      )}

      {(stage === "chat" || (stage === "verdict" && showTranscript)) && (
        <div className="interview-chat">
          <div className="interview-messages">
            {messages.map((message, index) => (
              <div
                className={`chat-message ${message.role}`}
                key={`${message.at}-${index}`}
              >
                {message.role === "assistant" && (
                  <div className="chat-avatar">
                    <Sparkles size={14} />
                  </div>
                )}
                <div className="chat-bubble">
                  {message.role === "assistant" ? (
                    <Markdown text={message.content} deckId="" />
                  ) : (
                    <p>{message.content}</p>
                  )}
                </div>
              </div>
            ))}

            {sending && (
              <div className="chat-message assistant">
                <div className="chat-avatar">
                  <Sparkles size={14} />
                </div>
                <div className="chat-bubble streaming">
                  {streamText ? (
                    <Markdown text={streamText} deckId="" />
                  ) : (
                    <span className="typing-dots">
                      <i />
                      <i />
                      <i />
                    </span>
                  )}
                </div>
              </div>
            )}

            {error && stage === "chat" && (
              <div className="inline-error chat-error">{error}</div>
            )}
            <div ref={bottomRef} />
          </div>

          {stage === "chat" && (
            <div className="interview-composer">
              {readOnly ? (
                <div className="interview-readonly">
                  Это собеседование доступно только для просмотра.
                </div>
              ) : (
                <>
                  <textarea
                    value={input}
                    onChange={(event) => setInput(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        void send();
                      }
                    }}
                    rows={2}
                    placeholder="Ответьте своими словами…"
                    disabled={sending || finishing}
                  />
                  <div className="composer-actions">
                    <button
                      className="ghost-btn small"
                      onClick={() => void send("Не знаю, подскажите")}
                      disabled={sending || finishing}
                    >
                      Не знаю
                    </button>
                    <span className="composer-hint">
                      Enter — отправить · Shift+Enter — новая строка
                    </span>
                    {sending ? (
                      <button
                        className="ghost-btn"
                        onClick={() => void stop()}
                        title="Остановить ответ"
                      >
                        <Square size={14} /> Стоп
                      </button>
                    ) : (
                      <button
                        className="primary-btn"
                        onClick={() => void send()}
                        disabled={!input.trim() || finishing}
                      >
                        <Send size={16} /> Отправить
                      </button>
                    )}
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}

      {stage === "verdict" && !showTranscript && verdict && (
        <div className="interview-scroll">
          {finishing && (
            <div className="interview-overlay">
              <Loader2 size={22} className="spin" />
              Анализирую ответы и собираю вердикт…
            </div>
          )}
          <div className="verdict">
            <div className={`verdict-level ${verdict.level}`}>
              <Sparkles size={18} />
              <span>{levelLabel(verdict.level)}</span>
            </div>
            <h2>Вердикт</h2>
            <p className="verdict-summary">{verdict.summary}</p>

            {verdict.strengths.length > 0 && (
              <section className="verdict-section">
                <h3>
                  <ThumbsUp size={15} /> Что получилось
                </h3>
                <ul>
                  {verdict.strengths.map((item, index) => (
                    <li key={`${item}-${index}`}>{item}</li>
                  ))}
                </ul>
              </section>
            )}

            {verdict.gaps.length > 0 && (
              <section className="verdict-section">
                <h3>
                  <AlertTriangle size={15} /> Где пробелы
                </h3>
                <div className="gap-list">
                  {verdict.gaps.map((gap, index) => (
                    <div className={`gap ${gap.severity}`} key={`${gap.topic}-${index}`}>
                      <div className="gap-head">
                        <strong>{gap.topic}</strong>
                        <span>{severityLabel(gap.severity)}</span>
                      </div>
                      <p>{gap.detail}</p>
                    </div>
                  ))}
                </div>
              </section>
            )}

            {verdict.toReview.length > 0 && (
              <section className="verdict-section">
                <h3>Что повторить</h3>
                <div className="verdict-chips">
                  {verdict.toReview.map((item, index) => (
                    <span className="verdict-chip" key={`${item}-${index}`}>
                      {item}
                    </span>
                  ))}
                </div>
              </section>
            )}

            {verdict.nextQuestions.length > 0 && (
              <section className="verdict-section">
                <h3>Вопросы, которые стоит уметь отвечать</h3>
                <ul className="next-questions">
                  {verdict.nextQuestions.map((item, index) => (
                    <li key={`${item}-${index}`}>{item}</li>
                  ))}
                </ul>
              </section>
            )}

            {error && <div className="inline-error">{error}</div>}

            {!readOnly && (
              <div className="verdict-actions">
                {showObjection ? (
                  <div className="objection-box">
                    <textarea
                      value={objection}
                      onChange={(event) => setObjection(event.target.value)}
                      rows={3}
                      placeholder="С чем вы не согласны? Например: «Я знаю эту тему, просто не понял вопрос»."
                    />
                    <div className="objection-actions">
                      <button
                        className="ghost-btn"
                        onClick={() => setShowObjection(false)}
                        disabled={finishing}
                      >
                        Отмена
                      </button>
                      <button
                        className="primary-btn"
                        onClick={() => void finish(true)}
                        disabled={finishing || !objection.trim()}
                      >
                        {finishing ? (
                          <Loader2 size={16} className="spin" />
                        ) : (
                          <MessageSquare size={16} />
                        )}
                        Обсудить и обновить вердикт
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <button
                      className="ghost-btn"
                      onClick={() => setShowObjection(true)}
                    >
                      <MessageSquare size={16} /> Поспорить
                    </button>
                    <button className="primary-btn" onClick={() => void generateCards()}>
                      {loadingCards ? (
                        <Loader2 size={16} className="spin" />
                      ) : (
                        <Sparkles size={16} />
                      )}
                      Создать карточки по пробелам
                    </button>
                  </>
                )}
              </div>
            )}
            {readOnly && (
              <div className="interview-readonly">
                Сессия недоступна для продолжения. Можно только просмотреть
                историю.
              </div>
            )}
          </div>
        </div>
      )}

      {stage === "cards" && (
        <div className="interview-scroll">
          <div className="cards-panel">
            <div className="cards-head">
              <div>
                <div className="eyebrow">После собеседования</div>
                <h2>Карточки по пробелам</h2>
                <p>
                  Проверьте формулировки и выберите, что добавить. Дубли будут
                  пропущены.
                </p>
              </div>
              <button
                className="ghost-btn small"
                onClick={() => setStage("verdict")}
              >
                <ArrowLeft size={14} /> К вердикту
              </button>
            </div>

            {createdInfo ? (
              <div className="cards-success">
                <div className="cards-success-icon">
                  <Check size={22} />
                </div>
                <h3>Готово</h3>
                <p>
                  Добавлено карточек: <strong>{createdInfo.created}</strong>
                  {createdInfo.skipped > 0 &&
                    `, пропущено дублей: ${createdInfo.skipped}`}
                  . Колода: <strong>{deckTitle(vault, createdInfo.deckId)}</strong>
                </p>
                <div className="cards-success-actions">
                  {createdInfo.created > 0 && (
                    <button
                      className="primary-btn"
                      onClick={() => {
                        onStartReview(
                          { kind: "deck", id: createdInfo.deckId },
                          deckTitle(vault, createdInfo.deckId),
                          "scheduled",
                          createdInfo.created,
                        );
                        onClose();
                      }}
                    >
                      <Play size={16} /> Повторить новые карточки
                    </button>
                  )}
                  <button className="ghost-btn" onClick={onClose}>
                    Закрыть
                  </button>
                </div>
              </div>
            ) : (
              <>
                <div className="cards-destination">
                  <label className="field">
                    <span>Куда добавить</span>
                    <select
                      value={destination}
                      onChange={(event) => setDestination(event.target.value)}
                    >
                      {scopeDecks.map((deck) => (
                        <option value={deck.id} key={deck.id}>
                          {deck.name}
                        </option>
                      ))}
                      <option value="__new__">Новая колода…</option>
                    </select>
                  </label>
                  {destination === "__new__" && (
                    <label className="field">
                      <span>Название новой колоды</span>
                      <input
                        value={newDeckName}
                        onChange={(event) => setNewDeckName(event.target.value)}
                        placeholder="AI-разбор"
                      />
                    </label>
                  )}
                </div>

                <div className="cards-list">
                  {cards.map((card, index) => {
                    const duplicate = duplicates.has(normalizeKey(card.front));
                    return (
                      <div
                        className={`card-editor ${card.enabled ? "" : "off"}`}
                        key={`${index}-${card.topic}`}
                      >
                        <div className="card-editor-head">
                          <label className="card-toggle">
                            <input
                              type="checkbox"
                              checked={card.enabled}
                              onChange={(event) =>
                                updateCard(index, { enabled: event.target.checked })
                              }
                            />
                            <span>Добавить</span>
                          </label>
                          <span className="card-topic">
                            {card.topic || "Пробел"}
                          </span>
                          {duplicate && (
                            <span className="card-duplicate">уже есть</span>
                          )}
                          <button
                            className="icon-btn"
                            title="Убрать карточку"
                            onClick={() => updateCard(index, { enabled: false })}
                          >
                            <Trash2 size={14} />
                          </button>
                        </div>
                        <label className="field">
                          <span>Вопрос</span>
                          <textarea
                            value={card.front}
                            onChange={(event) =>
                              updateCard(index, { front: event.target.value })
                            }
                            rows={2}
                          />
                        </label>
                        <label className="field">
                          <span>Ответ</span>
                          <textarea
                            value={card.back}
                            onChange={(event) =>
                              updateCard(index, { back: event.target.value })
                            }
                            rows={3}
                          />
                        </label>
                        <label className="field">
                          <span>Теги (через запятую)</span>
                          <input
                            value={card.tagsText}
                            onChange={(event) =>
                              updateCard(index, { tagsText: event.target.value })
                            }
                          />
                        </label>
                      </div>
                    );
                  })}
                </div>

                {error && <div className="inline-error">{error}</div>}

                <div className="cards-actions">
                  <span className="settings-hint">
                    Выбрано: {selectedCards.length} из {cards.length}
                  </span>
                  <button
                    className="primary-btn"
                    onClick={() => void commit()}
                    disabled={committing || selectedCards.length === 0}
                  >
                    {committing ? (
                      <Loader2 size={16} className="spin" />
                    ) : (
                      <Check size={16} />
                    )}
                    Добавить {selectedCards.length} карточек
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {finishing && stage === "verdict" && (
        <div className="interview-bottom-bar">
          <Loader2 size={16} className="spin" /> Анализирую ответы…
        </div>
      )}

      {stage === "chat" && error && (
        <div className="interview-bottom-bar">
          <AlertTriangle size={16} /> {error}
          <button className="ghost-btn small" onClick={() => setError(null)}>
            Скрыть
          </button>
        </div>
      )}
    </div>
  );
}


