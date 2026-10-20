import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Layers, Loader2, Plus, RefreshCw } from "lucide-react";
import { Sidebar } from "./components/Sidebar";
import { DeckView } from "./components/DeckView";
import { Review } from "./components/Review";
import { api } from "./lib/api";
import type { VaultInfo } from "./lib/types";

export default function App() {
  const [vault, setVault] = useState<VaultInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [fatal, setFatal] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [mode, setMode] = useState<"deck" | "review">("deck");
  const [syncing, setSyncing] = useState(false);
  const [version, setVersion] = useState(0);
  const [newDeck, setNewDeck] = useState("");
  const [creating, setCreating] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const apply = useCallback((info: VaultInfo) => {
    setVault(info);
    setSelectedId((prev) =>
      prev && info.decks.some((deck) => deck.id === prev)
        ? prev
        : (info.decks[0]?.id ?? null),
    );
  }, []);

  useEffect(() => {
    let alive = true;
    api
      .initVault()
      .then((info) => {
        if (alive) {
          apply(info);
          setFatal(null);
        }
      })
      .catch((err) => {
        if (alive) setFatal(String(err));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [apply]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 2400);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const refresh = useCallback(async () => {
    try {
      const info = await api.vaultInfo();
      apply(info);
    } catch (err) {
      setNotice(String(err));
    }
  }, [apply]);

  const openFolder = useCallback(async () => {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen !== "string") return;
    try {
      const info = await api.openVault(chosen);
      apply(info);
      setMode("deck");
      setVersion((value) => value + 1);
      setFatal(null);
      setNotice("Хранилище открыто");
    } catch (err) {
      setNotice(String(err));
    }
  }, [apply]);

  const sync = useCallback(async () => {
    setSyncing(true);
    try {
      const info = await api.syncVault();
      apply(info);
      setVersion((value) => value + 1);
      setNotice("Файлы перечитаны");
    } catch (err) {
      setNotice(String(err));
    } finally {
      setSyncing(false);
    }
  }, [apply]);

  const createDeck = useCallback(
    async (name: string) => {
      const info = await api.createDeck(name);
      apply(info);
      const created = info.decks.find((deck) => deck.name === name.trim());
      if (created) {
        setSelectedId(created.id);
        setMode("deck");
      }
    },
    [apply],
  );

  const selected = vault?.decks.find((deck) => deck.id === selectedId) ?? null;

  if (loading) {
    return (
      <div className="splash">
        <div className="splash-logo">c</div>
        <div className="splash-text">Открываем хранилище…</div>
        <Loader2 size={18} className="spin splash-spinner" />
      </div>
    );
  }

  if (fatal && !vault) {
    return (
      <div className="splash">
        <div className="splash-logo error">!</div>
        <h1>Не удалось открыть хранилище</h1>
        <p className="splash-error">{fatal}</p>
        <div className="splash-actions">
          <button className="ghost-btn" onClick={() => void openFolder()}>
            <FolderOpen size={16} /> Выбрать папку
          </button>
          <button className="primary-btn" onClick={() => window.location.reload()}>
            <RefreshCw size={16} /> Повторить
          </button>
        </div>
      </div>
    );
  }

  if (!vault) return null;

  if (mode === "review" && selected) {
    return (
      <Review
        deck={selected}
        onExit={() => {
          setMode("deck");
          void refresh();
        }}
        onReviewed={() => void refresh()}
      />
    );
  }

  return (
    <div className="app">
      <Sidebar
        vault={vault}
        selectedId={selectedId}
        onSelect={(id) => {
          setSelectedId(id);
          setMode("deck");
        }}
        onCreateDeck={async (name) => {
          await createDeck(name);
        }}
        onOpenVault={() => void openFolder()}
        onSync={() => void sync()}
        syncing={syncing}
      />

      {selected ? (
        <DeckView
          key={selected.id}
          deck={selected}
          version={version}
          onStartReview={() => setMode("review")}
          onRefresh={refresh}
        />
      ) : (
        <main className="content">
          <div className="welcome">
            <div className="welcome-mark">
              <Layers size={28} />
            </div>
            <h1>Создайте первую колоду</h1>
            <p>
              Колода — это папка с файлами <code>.crd</code> и медиа. Её удобно
              версионировать в git и переносить архивом.
            </p>
            <form
              className="welcome-form"
              onSubmit={async (event) => {
                event.preventDefault();
                if (!newDeck.trim() || creating) return;
                setCreating(true);
                try {
                  await createDeck(newDeck);
                  setNewDeck("");
                } catch (err) {
                  setNotice(String(err));
                } finally {
                  setCreating(false);
                }
              }}
            >
              <input
                value={newDeck}
                onChange={(event) => setNewDeck(event.target.value)}
                placeholder="Например: Испанский"
              />
              <button className="primary-btn" disabled={creating}>
                {creating ? (
                  <Loader2 size={16} className="spin" />
                ) : (
                  <Plus size={16} />
                )}
                Создать
              </button>
            </form>
            <button
              className="link-btn"
              onClick={() => void openFolder()}
            >
              Или открыть существующую папку
            </button>
            <div className="welcome-path">{vault.root}</div>
          </div>
        </main>
      )}

      {notice && <div className="toast">{notice}</div>}
    </div>
  );
}
