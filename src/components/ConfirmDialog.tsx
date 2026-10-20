import { useEffect, type ReactNode } from "react";
import { Loader2, Trash2 } from "lucide-react";

export function ConfirmDialog({
  title,
  message,
  confirmLabel = "Удалить",
  cancelLabel = "Отмена",
  busy = false,
  error,
  icon,
  onConfirm,
  onCancel,
}: {
  title: string;
  message: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  busy?: boolean;
  error?: string | null;
  icon?: ReactNode;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onCancel();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [busy, onCancel]);

  return (
    <div
      className="confirm-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
    >
      <div
        className="confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
      >
        <div className="confirm-icon">{icon ?? <Trash2 size={18} />}</div>
        <h3 id="confirm-title">{title}</h3>
        <p className="confirm-message">{message}</p>
        {error && <div className="inline-error">{error}</div>}
        <div className="confirm-actions">
          <button className="ghost-btn" onClick={onCancel} disabled={busy}>
            {cancelLabel}
          </button>
          <button className="danger-btn solid" onClick={onConfirm} disabled={busy}>
            {busy ? (
              <Loader2 size={15} className="spin" />
            ) : (
              <Trash2 size={15} />
            )}
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
