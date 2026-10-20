import { useEffect, useState } from "react";
import {
  AlertTriangle,
  ArrowDownToLine,
  Check,
  GitBranch,
  Link2,
  Loader2,
  RefreshCw,
  Unplug,
  X,
} from "lucide-react";
import { api } from "../lib/api";
import type { SyncStatus } from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";

function formatLast(value: number | null): string {
  if (!value) return "ещё не было";
  return new Date(value).toLocaleString("ru-RU", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function SyncPanel({
  initial,
  onClose,
  onStatus,
  onChanged,
}: {
  initial: SyncStatus | null;
  onClose: () => void;
  onStatus: (status: SyncStatus) => void;
  onChanged: () => void;
}) {
  const [status, setStatus] = useState<SyncStatus | null>(initial);
  const [remote, setRemote] = useState(initial?.remote ?? "");
  const [branch, setBranch] = useState(initial?.branch ?? "main");
  const [autoSync, setAutoSync] = useState(initial?.autoSync ?? true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(!initial);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);

  useEffect(() => {
    let alive = true;
    api
      .syncStatus()
      .then((next) => {
        if (!alive) return;
        setStatus(next);
        onStatus(next);
        setRemote(next.remote ?? "");
        setBranch(next.branch || "main");
        setAutoSync(next.configured ? next.autoSync : true);
      })
      .catch((err) => {
        if (alive) setError(String(err));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const apply = (next: SyncStatus) => {
    setStatus(next);
    onStatus(next);
  };

  const run = async <T,>(action: () => Promise<T>, after?: (value: T) => void) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await action();
      after?.(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const connect = () =>
    run(
      () => api.syncConnect(remote.trim(), branch.trim(), autoSync),
      (next) => {
        apply(next);
        setAutoSync(next.autoSync);
        if (next.conflicts.length === 0) onChanged();
      },
    );

  const syncNow = () =>
    run(
      () => api.syncNow(),
      (next) => {
        apply(next);
        if (next.conflicts.length === 0) onChanged();
      },
    );

  const pull = () =>
    run(
      () => api.syncPull(),
      (next) => {
        apply(next);
        if (next.conflicts.length === 0) onChanged();
      },
    );

  const resolve = (path: string, side: "local" | "remote") =>
    run(
      () => api.syncResolve(path, side),
      (next) => {
        apply(next);
        if (next.conflicts.length === 0) onChanged();
      },
    );

  const abort = () =>
    run(
      () => api.syncAbort(),
      (next) => {
        apply(next);
        onChanged();
      },
    );

  const disconnect = () =>
    run(
      () => api.syncDisconnect(),
      (next) => {
        apply(next);
        setConfirmDisconnect(false);
        setAutoSync(true);
      },
    );

  const connected = status?.configured ?? false;
  const conflicts = status?.conflicts ?? [];
  const displayError = error ?? status?.error ?? null;
  const gitMissing = status ? !status.gitAvailable : false;

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div className="modal sync-modal">
        <header className="editor-head">
          <div>
            <div className="eyebrow">Git</div>
            <h2>Синхронизация</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Закрыть">
            <X size={18} />
          </button>
        </header>

        <div className="sync-body">
          {loading ? (
            <div className="sync-loading">
              <Loader2 size={20} className="spin" /> Проверяем состояние…
            </div>
          ) : (
            <>
              {gitMissing && (
                <div className="sync-warning">
                  <AlertTriangle size={15} /> Git не найден в системе. Установите
                  Git и перезапустите приложение.
                </div>
              )}

              {!connected ? (
                <>
                  <p className="sync-intro">
                    Укажите репозиторий, в котором будет храниться этот vault.
                    Один vault — один репозиторий. Контент колод синхронизируется,
                    прогресс повторений остаётся локальным.
                  </p>
                  <label className="field">
                    <span>URL репозитория</span>
                    <input
                      value={remote}
                      onChange={(event) => setRemote(event.target.value)}
                      placeholder="https://github.com/user/cards.git"
                      disabled={busy}
                    />
                  </label>
                  <div className="sync-form-row">
                    <label className="field">
                      <span>Ветка</span>
                      <input
                        value={branch}
                        onChange={(event) => setBranch(event.target.value)}
                        placeholder="main"
                        disabled={busy}
                      />
                    </label>
                    <label className="sync-check">
                      <input
                        type="checkbox"
                        checked={autoSync}
                        onChange={(event) => setAutoSync(event.target.checked)}
                        disabled={busy}
                      />
                      <span>Автосинк после изменений</span>
                    </label>
                  </div>
                  <p className="sync-hint">
                    Авторизация — через системный Git (Credential Manager/SSH).
                    При первом подключении GitHub может попросить доступ.
                  </p>
                  <div className="sync-actions">
                    <button
                      className="primary-btn"
                      onClick={() => void connect()}
                      disabled={busy || gitMissing || !remote.trim()}
                    >
                      {busy ? (
                        <Loader2 size={16} className="spin" />
                      ) : (
                        <Link2 size={16} />
                      )}
                      Подключить
                    </button>
                  </div>
                </>
              ) : (
                <>
                  <div className="sync-connection">
                    <div>
                      <span>Репозиторий</span>
                      <code>{status?.remote}</code>
                    </div>
                    <div>
                      <span>Ветка</span>
                      <code>{status?.branch}</code>
                    </div>
                    <div>
                      <span>Автосинк</span>
                      <strong>{status?.autoSync ? "включён" : "выключен"}</strong>
                    </div>
                    <div>
                      <span>Последняя синхронизация</span>
                      <strong>{formatLast(status?.lastSync ?? null)}</strong>
                    </div>
                  </div>

                  <div className="sync-flags">
                    {status?.syncing && (
                      <span className="sync-flag accent">
                        <Loader2 size={12} className="spin" /> идёт синхронизация
                      </span>
                    )}
                    {status && status.dirty && (
                      <span className="sync-flag">есть локальные изменения</span>
                    )}
                    {status && status.ahead > 0 && (
                      <span className="sync-flag">впереди на {status.ahead}</span>
                    )}
                    {status && status.behind > 0 && (
                      <span className="sync-flag">позади на {status.behind}</span>
                    )}
                    {status &&
                      !status.dirty &&
                      status.ahead === 0 &&
                      status.behind === 0 &&
                      conflicts.length === 0 && (
                        <span className="sync-flag ok">
                          <Check size={12} /> синхронизировано
                        </span>
                      )}
                  </div>

                  {conflicts.length > 0 && (
                    <div className="sync-conflicts">
                      <div className="sync-warning">
                        <AlertTriangle size={15} /> Конфликты: выберите, какую
                        версию оставить. Пока конфликты не разрешены, автосинк
                        приостановлен.
                      </div>
                      {conflicts.map((path) => (
                        <div className="sync-conflict" key={path}>
                          <code>{path}</code>
                          <div className="sync-conflict-actions">
                            <button
                              className="ghost-btn small"
                              onClick={() => void resolve(path, "local")}
                              disabled={busy}
                            >
                              Моё
                            </button>
                            <button
                              className="ghost-btn small"
                              onClick={() => void resolve(path, "remote")}
                              disabled={busy}
                            >
                              Удалённое
                            </button>
                          </div>
                        </div>
                      ))}
                      <button
                        className="ghost-btn small"
                        onClick={() => void abort()}
                        disabled={busy}
                      >
                        <X size={14} /> Прервать merge
                      </button>
                    </div>
                  )}

                  <div className="sync-actions">
                    <button
                      className="primary-btn"
                      onClick={() => void syncNow()}
                      disabled={busy || conflicts.length > 0}
                    >
                      {busy ? (
                        <Loader2 size={16} className="spin" />
                      ) : (
                        <RefreshCw size={16} />
                      )}
                      Синхронизировать
                    </button>
                    <button
                      className="ghost-btn"
                      onClick={() => void pull()}
                      disabled={busy || conflicts.length > 0}
                    >
                      <ArrowDownToLine size={16} /> Получить
                    </button>
                    <span className="sync-spacer" />
                    <button
                      className="ghost-btn"
                      onClick={() => setConfirmDisconnect(true)}
                      disabled={busy}
                    >
                      <Unplug size={15} /> Отключить
                    </button>
                  </div>

                  <div className="sync-hint">
                    <GitBranch size={13} /> Локальный git-репозиторий:{" "}
                    <code>.git</code> в папке vault. Отключение не удаляет файлы и
                    историю.
                  </div>
                </>
              )}

              {displayError && <div className="inline-error">{displayError}</div>}
            </>
          )}
        </div>
      </div>

      {confirmDisconnect && (
        <ConfirmDialog
          title="Отключить синхронизацию?"
          message="Локальные файлы vault и git-история останутся на месте. Автосинк будет выключен."
          confirmLabel="Отключить"
          icon={<Unplug size={18} />}
          busy={busy}
          error={error}
          onCancel={() => setConfirmDisconnect(false)}
          onConfirm={() => void disconnect()}
        />
      )}
    </div>
  );
}
