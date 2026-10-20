import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Check,
  Loader2,
  Minus,
  Plus,
  RotateCcw,
  Send,
  Sparkles,
  Square,
  Trash2,
  X,
} from "lucide-react";
import { api } from "../lib/api";
import { Markdown } from "../lib/markdown";
import type {
  AiEvent,
  AssistantChat,
  AssistantMessage,
  CardBatch,
  VaultInfo,
} from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";

const ACTION_START = "<<<CARDZ_ACTIONS>>>";

function visibleStream(text: string): string {
  const start = text.indexOf(ACTION_START);
  if (start >= 0) return text.slice(0, start).replace(/```(?:json)?\s*$/, "").trimEnd();
  const partial = text.lastIndexOf("<<<");
  if (partial >= 0 && ACTION_START.startsWith(text.slice(partial))) {
    return text.slice(0, partial).trimEnd();
  }
  return text;
}

function deckName(vault: VaultInfo, id: string | null): string | null {
  if (!id) return null;
  return vault.decks.find((deck) => deck.id === id)?.name ?? id;
}

export function AssistantDock({
  vault,
  currentDeckId,
  expandSignal,
  onChanged,
  onOpenSettings,
}: {
  vault: VaultInfo;
  currentDeckId: string | null;
  expandSignal: number;
  onChanged: () => void;
  onOpenSettings: () => void;
}) {
  const [chat, setChat] = useState<AssistantChat | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [minimized, setMinimized] = useState(false);
  const [loading, setLoading] = useState(true);
  const [sending, setSending] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [unread, setUnread] = useState(0);
  const [confirmNew, setConfirmNew] = useState(false);
  const [undoBatch, setUndoBatch] = useState<CardBatch | null>(null);
  const [pendingDeck, setPendingDeck] = useState("");
  const [pendingNewDeck, setPendingNewDeck] = useState("");
  const [pendingBusy, setPendingBusy] = useState(false);
  const [pendingError, setPendingError] = useState<string | null>(null);

  const bottomRef = useRef<HTMLDivElement | null>(null);
  const activeSendRef = useRef(false);
  const assistantIdRef = useRef<string | null>(null);
  const textPartIdRef = useRef<string | null>(null);
  const streamRef = useRef("");
  const sawDeltaRef = useRef(false);

  const pending = chat?.pending ?? null;
  const currentDeckName = deckName(vault, currentDeckId);

  useEffect(() => {
    if (expandSignal > 0) {
      setMinimized(false);
      setUnread(0);
    }
  }, [expandSignal]);

  useEffect(() => {
    let alive = true;
    const boot = async () => {
      setLoading(true);
      try {
        const result = await api.aiAssistantChatPrepare();
        if (!alive) return;
        setChat(result.chat);
        setSessionId(result.sessionId);
      } catch (err) {
        if (alive) setError(String(err));
      } finally {
        if (alive) setLoading(false);
      }
    };
    void boot();
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!pending) return;
    setPendingDeck(
      pending.suggestedDeckId ??
        (currentDeckId && vault.decks.some((deck) => deck.id === currentDeckId)
          ? currentDeckId
          : vault.decks[0]?.id ?? "__new__"),
    );
    setPendingNewDeck("AI-разбор");
    setPendingError(null);
  }, [pending, currentDeckId, vault.decks]);

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
                id?: string;
                sessionID?: string;
                error?: { data?: { message?: string } } | null;
              }
            | undefined;
          if (!info || info.sessionID !== sessionId) return;
          if (info.role === "assistant" && !assistantIdRef.current && info.id) {
            assistantIdRef.current = info.id;
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
              setStreamText(visibleStream(streamRef.current));
            }
          }
          return;
        }
        if (payload.type === "message.part.delta") {
          if (properties.sessionID !== sessionId) return;
          if (properties.field !== "text") return;
          if (properties.messageID !== assistantIdRef.current) return;
          if (properties.partID !== textPartIdRef.current) return;
          const delta =
            typeof properties.delta === "string" ? properties.delta : "";
          if (!delta) return;
          sawDeltaRef.current = true;
          streamRef.current += delta;
          setStreamText(visibleStream(streamRef.current));
        }
      });
      if (!alive) unlisten?.();
    })();
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [sessionId]);

  useEffect(() => {
    if (!minimized) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
    }
  }, [chat?.messages, streamText, minimized, sending]);

  const send = async (forced?: string) => {
    const text = (forced ?? input).trim();
    if (!text || sending || !chat) return;
    if (!forced) setInput("");
    setError(null);
    setSending(true);
    setStreamText("");
    streamRef.current = "";
    sawDeltaRef.current = false;
    assistantIdRef.current = null;
    textPartIdRef.current = null;
    activeSendRef.current = true;
    setChat((prev) =>
      prev
        ? {
            ...prev,
            messages: [
              ...prev.messages,
              {
                role: "user" as const,
                content: text,
                at: Date.now(),
                batch: null,
              },
            ],
          }
        : prev,
    );
    try {
      const result = await api.aiAssistantChatSend(text, currentDeckId);
      setChat(result.chat);
      setSessionId(result.sessionId);
      setStreamText("");
      onChanged();
      if (minimized) setUnread((value) => value + 1);
    } catch (err) {
      setError(String(err));
    } finally {
      activeSendRef.current = false;
      setSending(false);
      streamRef.current = "";
    }
  };

  const stop = async () => {
    if (!sessionId) return;
    await api.aiInterviewAbort(sessionId).catch(() => undefined);
  };

  const newChat = async () => {
    if (sending) return;
    setConfirmNew(false);
    setLoading(true);
    setError(null);
    setStreamText("");
    try {
      const fresh = await api.aiAssistantChatNew();
      setChat(fresh);
      setSessionId(null);
      const result = await api.aiAssistantChatPrepare();
      setChat(result.chat);
      setSessionId(result.sessionId);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const confirmPending = async () => {
    if (!pending || pendingBusy) return;
    setPendingBusy(true);
    setPendingError(null);
    try {
      let deckId = pendingDeck;
      if (deckId === "__new__") {
        const name = pendingNewDeck.trim();
        if (!name) {
          setPendingError("Введите название новой колоды");
          setPendingBusy(false);
          return;
        }
        const result = await api.createDeck(name);
        deckId = result.deckId;
      }
      const updated = await api.aiAssistantChatConfirm(deckId);
      setChat(updated);
      onChanged();
    } catch (err) {
      setPendingError(String(err));
    } finally {
      setPendingBusy(false);
    }
  };

  const cancelPending = async () => {
    if (pendingBusy) return;
    setPendingBusy(true);
    try {
      const updated = await api.aiAssistantChatCancelPending();
      setChat(updated);
    } catch (err) {
      setPendingError(String(err));
    } finally {
      setPendingBusy(false);
    }
  };

  const undo = async () => {
    if (!undoBatch) return;
    try {
      const updated = await api.aiAssistantChatUndo(undoBatch.id);
      setChat(updated);
      onChanged();
      setUndoBatch(null);
    } catch (err) {
      setError(String(err));
    }
  };

  const messages = chat?.messages ?? [];
  const showStream = sending;

  if (minimized) {
    return (
      <>
        <button
          className="assistant-fab"
          onClick={() => {
            setMinimized(false);
            setUnread(0);
          }}
          title="Открыть ИИ-ассистента"
        >
          {sending ? (
            <Loader2 size={20} className="spin" />
          ) : (
            <Sparkles size={20} />
          )}
          {unread > 0 && <span className="assistant-fab-badge">{unread}</span>}
        </button>
        {undoBatch && (
          <ConfirmDialog
            title="Отменить создание карточек?"
            message={`Карточки будут удалены из колоды «${undoBatch.deckName}».`}
            confirmLabel="Отменить создание"
            icon={<Trash2 size={18} />}
            onCancel={() => setUndoBatch(null)}
            onConfirm={() => void undo()}
          />
        )}
      </>
    );
  }

  return (
    <>
      <aside className="assistant-dock">
        <header className="assistant-head">
          <div className="assistant-identity">
            <span className="assistant-mark">
              <Sparkles size={15} />
            </span>
            <div>
              <strong>ИИ-ассистент</strong>
              <span>
                {currentDeckName ? `Сейчас: ${currentDeckName}` : "Колода не выбрана"}
              </span>
            </div>
          </div>
          <div className="assistant-head-actions">
            <button
              className="icon-btn"
              onClick={() => setConfirmNew(true)}
              title="Новый чат"
              disabled={sending}
            >
              <Plus size={15} />
            </button>
            <button
              className="icon-btn"
              onClick={() => setMinimized(true)}
              title="Свернуть"
            >
              <Minus size={15} />
            </button>
          </div>
        </header>

        <div className="assistant-messages">
          {loading && (
            <div className="assistant-loading">
              <Loader2 size={18} className="spin" /> Подключаю ассистента…
            </div>
          )}

          {!loading && messages.length === 0 && (
            <div className="assistant-welcome">
              <Sparkles size={20} />
              <p>
                Привет! Попроси создать или исправить карточки. Например:
                <br />
                <em>«Сделай карточку: что такое p95 и p99 перцентили»</em>
                <br />
                <em>«Добавь 5 карточек про перцентили и положи в Статистику»</em>
              </p>
            </div>
          )}

          {messages.map((message, index) => (
            <AssistantLine
              key={`${message.at}-${index}`}
              message={message}
              onUndo={setUndoBatch}
            />
          ))}

          {showStream && (
            <div className="chat-message assistant assistant-inline">
              <div className="chat-avatar">
                <Sparkles size={13} />
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

          {error && (
            <div className="assistant-error">
              <AlertTriangle size={14} />
              <span>{error}</span>
              <button onClick={onOpenSettings}>Настройки</button>
            </div>
          )}
          <div ref={bottomRef} />
        </div>

        <div className="assistant-composer">
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
            placeholder="Что сделать с карточками?"
            disabled={sending || loading}
          />
          <div className="assistant-composer-actions">
            {sending ? (
              <button className="ghost-btn small" onClick={() => void stop()}>
                <Square size={13} /> Стоп
              </button>
            ) : (
              <span className="assistant-hint">
                Enter — отправить · можно указать колоду
              </span>
            )}
            <button
              className="primary-btn small"
              onClick={() => void send()}
              disabled={sending || loading || !input.trim()}
            >
              <Send size={15} /> Отправить
            </button>
          </div>
        </div>
      </aside>

      {pending && (
        <div className="assistant-pending-backdrop">
          <div className="assistant-pending">
            <header>
              <div>
                <div className="eyebrow">Нужен выбор</div>
                <h2>{pending.question || "Куда добавить карточки?"}</h2>
              </div>
              <button
                className="icon-btn"
                onClick={() => void cancelPending()}
                disabled={pendingBusy}
              >
                <X size={17} />
              </button>
            </header>
            <div className="pending-cards">
              {pending.cards.map((card, index) => (
                <div className="pending-card" key={`${card.front}-${index}`}>
                  <strong>{card.front}</strong>
                  <span>{card.back}</span>
                </div>
              ))}
            </div>
            <label className="field">
              <span>Колода</span>
              <select
                value={pendingDeck}
                onChange={(event) => setPendingDeck(event.target.value)}
                disabled={pendingBusy}
              >
                {vault.decks.map((deck) => (
                  <option value={deck.id} key={deck.id}>
                    {deck.name}
                  </option>
                ))}
                <option value="__new__">Новая колода…</option>
              </select>
            </label>
            {pendingDeck === "__new__" && (
              <label className="field">
                <span>Название новой колоды</span>
                <input
                  value={pendingNewDeck}
                  onChange={(event) => setPendingNewDeck(event.target.value)}
                  placeholder="AI-разбор"
                  disabled={pendingBusy}
                />
              </label>
            )}
            {pendingError && <div className="inline-error">{pendingError}</div>}
            <div className="assistant-pending-actions">
              <button
                className="ghost-btn"
                onClick={() => void cancelPending()}
                disabled={pendingBusy}
              >
                Отмена
              </button>
              <button
                className="primary-btn"
                onClick={() => void confirmPending()}
                disabled={pendingBusy}
              >
                {pendingBusy ? (
                  <Loader2 size={15} className="spin" />
                ) : (
                  <Check size={15} />
                )}
                Создать в этой колоде
              </button>
            </div>
          </div>
        </div>
      )}

      {confirmNew && (
        <ConfirmDialog
          title="Начать новый чат?"
          message="Текущая история ассистента будет очищена. Созданные карточки останутся в колодах."
          confirmLabel="Новый чат"
          icon={<RotateCcw size={18} />}
          onCancel={() => setConfirmNew(false)}
          onConfirm={() => void newChat()}
        />
      )}

      {undoBatch && (
        <ConfirmDialog
          title="Отменить создание карточек?"
          message={`Карточки будут удалены из колоды «${undoBatch.deckName}».`}
          confirmLabel="Отменить создание"
          icon={<Trash2 size={18} />}
          onCancel={() => setUndoBatch(null)}
          onConfirm={() => void undo()}
        />
      )}
    </>
  );
}

function AssistantLine({
  message,
  onUndo,
}: {
  message: AssistantMessage;
  onUndo: (batch: CardBatch) => void;
}) {
  if (message.role === "system") {
    if (message.batch) {
      const batch = message.batch;
      return (
        <div className={`assistant-batch ${batch.undone ? "undone" : ""}`}>
          <div className="assistant-batch-head">
            <Check size={14} />
            <strong>{message.content}</strong>
          </div>
          <div className="assistant-batch-cards">
            {batch.cards.map((card, index) => (
              <div className="assistant-batch-card" key={`${card.front}-${index}`}>
                <strong>{card.front}</strong>
                <span>{card.back}</span>
              </div>
            ))}
          </div>
          {!batch.undone && (
            <button
              className="ghost-btn small"
              onClick={() => onUndo(batch)}
            >
              <Trash2 size={13} /> Отменить
            </button>
          )}
        </div>
      );
    }
    return <div className="assistant-system">{message.content}</div>;
  }

  return (
    <div className={`chat-message ${message.role} assistant-inline`}>
      {message.role === "assistant" && (
        <div className="chat-avatar">
          <Sparkles size={13} />
        </div>
      )}
      <div className="chat-bubble">
        {message.role === "assistant" ? (
          <Markdown text={message.content} deckId="" />
        ) : (
          <p className="assistant-user-text">{message.content}</p>
        )}
      </div>
    </div>
  );
}



