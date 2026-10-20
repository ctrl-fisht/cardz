import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Check,
  Clock,
  History,
  Loader2,
  MessageSquare,
  RefreshCw,
  Save,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { api } from "../lib/api";
import type {
  AiSettings,
  AiStatus,
  InterviewSummary,
} from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";

function formatDate(value: number): string {
  return new Date(value).toLocaleString("ru-RU", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatDuration(seconds: number): string {
  if (!seconds) return "—";
  const minutes = Math.round(seconds / 60);
  return `${minutes} мин`;
}

function levelLabel(level: string | null): string {
  if (level === "weak") return "есть пробелы";
  if (level === "strong") return "уверенно";
  if (level === "ok") return "нужна практика";
  return "без вердикта";
}

export function SettingsPanel({
  onClose,
  onOpenInterview,
}: {
  onClose: () => void;
  onOpenInterview: (id: string) => void;
}) {
  const [tab, setTab] = useState<"ai" | "history">("ai");
  const [settings, setSettings] = useState<AiSettings | null>(null);
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [history, setHistory] = useState<InterviewSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);

  const loadHistory = async () => {
    setHistoryLoading(true);
    try {
      setHistory(await api.aiHistoryList());
      setHistoryError(null);
    } catch (err) {
      setHistoryError(String(err));
    } finally {
      setHistoryLoading(false);
    }
  };

  useEffect(() => {
    let alive = true;
    const boot = async () => {
      try {
        const [nextSettings, nextStatus] = await Promise.all([
          api.aiSettingsGet(),
          api.aiStatus(),
        ]);
        if (!alive) return;
        setSettings(nextSettings);
        setStatus(nextStatus);
        if (nextStatus.available) {
          setModelsLoading(true);
          api
            .aiModels()
            .then((list) => {
              if (alive) setModels(list);
            })
            .catch(() => undefined)
            .finally(() => {
              if (alive) setModelsLoading(false);
            });
        }
      } catch (err) {
        if (alive) setError(String(err));
      } finally {
        if (alive) setLoading(false);
      }
    };
    void boot();
    void loadHistory();
    return () => {
      alive = false;
    };
  }, []);

  const refreshStatus = async () => {
    setModelsLoading(true);
    try {
      const next = await api.aiStatus();
      setStatus(next);
      if (next.available) {
        const list = await api.aiModels();
        setModels(list);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setModelsLoading(false);
    }
  };

  const save = async () => {
    if (!settings || saving) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const next = await api.aiSettingsSave(settings);
      setSettings(next);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2200);
      await refreshStatus();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const remove = async (id: string) => {
    try {
      await api.aiHistoryDelete(id);
      await loadHistory();
    } catch (err) {
      setHistoryError(String(err));
    }
  };

  const clearHistory = async () => {
    if (clearing) return;
    setClearing(true);
    try {
      await api.aiHistoryClear();
      setConfirmClear(false);
      await loadHistory();
    } catch (err) {
      setHistoryError(String(err));
    } finally {
      setClearing(false);
    }
  };

  return (
    <div
      className="modal-backdrop settings-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !saving) onClose();
      }}
    >
      <div className="modal settings-modal">
        <header className="editor-head">
          <div>
            <div className="eyebrow">Cardz</div>
            <h2>Настройки</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Закрыть">
            <X size={18} />
          </button>
        </header>

        <div className="settings-tabs">
          <button
            className={`settings-tab ${tab === "ai" ? "on" : ""}`}
            onClick={() => setTab("ai")}
          >
            <Sparkles size={15} /> ИИ
          </button>
          <button
            className={`settings-tab ${tab === "history" ? "on" : ""}`}
            onClick={() => setTab("history")}
          >
            <History size={15} /> История
          </button>
        </div>

        <div className="settings-body">
          {tab === "ai" ? (
            loading ? (
              <div className="sync-loading">
                <Loader2 size={20} className="spin" /> Проверяем настройки…
              </div>
            ) : (
              <>
                <div className="ai-status-row">
                  <span
                    className={`ai-status-dot ${
                      status?.available ? "ok" : "bad"
                    }`}
                  />
                  <div>
                    <strong>OpenCode CLI</strong>
                    <span>
                      {status?.available
                        ? `найден${status.version ? ` · ${status.version}` : ""}`
                        : "не найден"}
                    </span>
                  </div>
                  <button
                    className="ghost-btn small"
                    onClick={() => void refreshStatus()}
                    disabled={modelsLoading}
                  >
                    <RefreshCw
                      size={14}
                      className={modelsLoading ? "spin" : ""}
                    />
                    Проверить
                  </button>
                </div>

                {status?.error && (
                  <div className="sync-warning">
                    <AlertTriangle size={15} /> {status.error}
                  </div>
                )}

                <p className="settings-hint">
                  Cardz запускает локальный OpenCode и использует вашу текущую
                  авторизацию. API-ключи в приложении не хранятся.
                </p>

                {settings && (
                  <>
                    <label className="field">
                      <span>Путь к OpenCode CLI</span>
                      <input
                        value={settings.cliPath ?? ""}
                        onChange={(event) =>
                          setSettings({
                            ...settings,
                            cliPath: event.target.value || null,
                          })
                        }
                        placeholder="Автоопределение"
                      />
                    </label>

                    <label className="field">
                      <span>Модель</span>
                      <input
                        list="cardz-ai-models"
                        value={settings.model}
                        onChange={(event) =>
                          setSettings({
                            ...settings,
                            model: event.target.value,
                          })
                        }
                        placeholder="opencode-go/deepseek-v4.1-flash"
                      />
                      <datalist id="cardz-ai-models">
                        {models.map((model) => (
                          <option key={model} value={model} />
                        ))}
                      </datalist>
                    </label>
                    <p className="settings-hint">
                      Формат: <code>провайдер/модель</code>.{" "}
                      {modelsLoading
                        ? "Загружаем список моделей…"
                        : `Доступно моделей: ${models.length || "нет данных"}.`}
                    </p>

                    <div className="settings-grid">
                      <label className="field">
                        <span>Длительность</span>
                        <select
                          value={settings.minutes}
                          onChange={(event) =>
                            setSettings({
                              ...settings,
                              minutes: Number(event.target.value),
                            })
                          }
                        >
                          <option value={10}>10 минут</option>
                          <option value={15}>15 минут</option>
                          <option value={20}>20 минут</option>
                          <option value={30}>30 минут</option>
                        </select>
                      </label>
                      <label className="field">
                        <span>Стиль</span>
                        <select
                          value={settings.style}
                          onChange={(event) =>
                            setSettings({
                              ...settings,
                              style: event.target.value as AiSettings["style"],
                            })
                          }
                        >
                          <option value="friendly">Дружелюбный</option>
                          <option value="strict">Строгий экзамен</option>
                          <option value="quick">Быстрый темп</option>
                        </select>
                      </label>
                    </div>

                    {error && <div className="inline-error">{error}</div>}

                    <div className="settings-actions">
                      {saved && (
                        <span className="settings-saved">
                          <Check size={14} /> Сохранено
                        </span>
                      )}
                      <button
                        className="primary-btn"
                        onClick={() => void save()}
                        disabled={saving}
                      >
                        {saving ? (
                          <Loader2 size={16} className="spin" />
                        ) : (
                          <Save size={16} />
                        )}
                        Сохранить
                      </button>
                    </div>
                  </>
                )}
              </>
            )
          ) : (
            <>
              <div className="history-head">
                <span className="settings-hint">
                  Собеседования хранятся локально в папке приложения и не
                  попадают в vault.
                </span>
                {history.length > 0 && (
                  <button
                    className="ghost-btn small"
                    onClick={() => setConfirmClear(true)}
                  >
                    <Trash2 size={14} /> Очистить
                  </button>
                )}
              </div>

              {historyError && <div className="inline-error">{historyError}</div>}

              {historyLoading ? (
                <div className="sync-loading">
                  <Loader2 size={20} className="spin" /> Загружаем историю…
                </div>
              ) : history.length === 0 ? (
                <div className="empty-state settings-empty">
                  <div className="empty-icon">
                    <MessageSquare size={22} />
                  </div>
                  <h3>Пока пусто</h3>
                  <p>
                    Выберите колоду или папку и нажмите «Чат с AI», чтобы начать
                    первое собеседование.
                  </p>
                </div>
              ) : (
                <div className="history-list">
                  {history.map((item) => (
                    <div className="history-row" key={item.id}>
                      <button
                        className="history-main"
                        onClick={() => onOpenInterview(item.id)}
                      >
                        <div className="history-title">{item.title}</div>
                        <div className="history-meta">
                          <span>{formatDate(item.updatedAt)}</span>
                          <span>
                            <Clock size={12} /> {formatDuration(item.durationSec)}
                          </span>
                          <span>{item.messageCount} сообщений</span>
                          <span className={`history-level ${item.level ?? ""}`}>
                            {levelLabel(item.level)}
                          </span>
                          {item.cardsCreated > 0 && (
                            <span>{item.cardsCreated} карточек</span>
                          )}
                        </div>
                      </button>
                      <button
                        className="icon-btn danger-icon"
                        title="Удалить из истории"
                        onClick={() => void remove(item.id)}
                      >
                        <Trash2 size={15} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      </div>

      {confirmClear && (
        <ConfirmDialog
          title="Очистить историю?"
          message="Все сохранённые собеседования, вердикты и предложенные карточки будут удалены. Колоды и созданные карточки останутся."
          confirmLabel="Очистить"
          icon={<Trash2 size={18} />}
          busy={clearing}
          error={historyError}
          onCancel={() => setConfirmClear(false)}
          onConfirm={() => void clearHistory()}
        />
      )}
    </div>
  );
}

