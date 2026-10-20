import { useState } from "react";
import {
  FolderOpen,
  Plus,
  RefreshCw,
  Layers,
  Loader2,
} from "lucide-react";
import type { VaultInfo } from "../lib/types";

function deckHue(id: string): number {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) {
    hash = (hash * 31 + id.charCodeAt(i)) % 360;
  }
  return hash;
}

export function Sidebar({
  vault,
  selectedId,
  onSelect,
  onCreateDeck,
  onOpenVault,
  onSync,
  syncing,
}: {
  vault: VaultInfo;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreateDeck: (name: string) => Promise<void>;
  onOpenVault: () => void;
  onSync: () => void;
  syncing: boolean;
}) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    const value = name.trim();
    if (!value || busy) return;
    setBusy(true);
    try {
      await onCreateDeck(value);
      setName("");
      setCreating(false);
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="logo">c</div>
        <div className="brand-text">
          <span className="brand-name">cardz</span>
          <span className="brand-sub">local-first</span>
        </div>
      </div>

      <div className="side-section">
        <div className="side-label">
          <span>Колоды</span>
          <button
            className="icon-btn small"
            onClick={() => setCreating((value) => !value)}
            title="Новая колода"
          >
            <Plus size={15} />
          </button>
        </div>

        <nav className="deck-list">
          {vault.decks.map((deck) => {
            const active = deck.id === selectedId;
            const badge = deck.counts.new + deck.counts.due;
            return (
              <button
                key={deck.id}
                className={`deck-item ${active ? "active" : ""}`}
                onClick={() => onSelect(deck.id)}
              >
                <span
                  className="deck-dot"
                  style={{
                    background: `hsl(${deckHue(deck.id)} 70% 62%)`,
                  }}
                />
                <span className="deck-name">{deck.name}</span>
                {badge > 0 && <span className="deck-badge">{badge}</span>}
              </button>
            );
          })}
          {vault.decks.length === 0 && !creating && (
            <div className="side-empty">Пока нет колод</div>
          )}
        </nav>

        {creating && (
          <form
            className="new-deck"
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            <input
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Название колоды"
              onKeyDown={(event) => {
                if (event.key === "Escape") setCreating(false);
              }}
            />
            <button type="submit" className="mini-primary" disabled={busy}>
              {busy ? <Loader2 size={14} className="spin" /> : "Создать"}
            </button>
          </form>
        )}
      </div>

      <div className="side-spacer" />

      <div className="side-footer">
        <div className="vault-card" title={vault.root}>
          <Layers size={15} />
          <div>
            <div className="vault-name">{vault.name}</div>
            <div className="vault-path">{vault.root}</div>
          </div>
        </div>
        <div className="side-actions">
          <button className="side-action" onClick={onOpenVault}>
            <FolderOpen size={14} /> Сменить папку
          </button>
          <button
            className="icon-btn"
            onClick={onSync}
            title="Перечитать файлы с диска"
            disabled={syncing}
          >
            <RefreshCw size={15} className={syncing ? "spin" : ""} />
          </button>
        </div>
      </div>
    </aside>
  );
}
