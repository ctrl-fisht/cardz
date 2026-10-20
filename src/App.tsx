import { useCallback, useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Layers, Loader2, Plus, RefreshCw } from "lucide-react";
import { Sidebar } from "./components/Sidebar";
import { DeckView } from "./components/DeckView";
import { GroupView } from "./components/GroupView";
import { Review } from "./components/Review";
import { api } from "./lib/api";
import type { Deck, ReviewTarget, StartReview, VaultInfo } from "./lib/types";

type Selection = { kind: "deck" | "group"; id: string };

export default function App() {
  const [vault, setVault] = useState<VaultInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [fatal, setFatal] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [reviewTarget, setReviewTarget] = useState<ReviewTarget | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [version, setVersion] = useState(0);
  const [newDeck, setNewDeck] = useState("");
  const [creating, setCreating] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const apply = useCallback((info: VaultInfo) => {
    setVault(info);
    setSelection((prev) => {
      if (prev?.kind === "deck" && info.decks.some((deck) => deck.id === prev.id)) {
        return prev;
      }
      if (
        prev?.kind === "group" &&
        info.groups.some((group) => group.id === prev.id)
      ) {
        return prev;
      }
      if (info.decks.length > 0) {
        return { kind: "deck", id: info.decks[0].id };
      }
      if (info.groups.length > 0) {
        return { kind: "group", id: info.groups[0].id };
      }
      return null;
    });
  }, []);

  useEffect(() => {
    let alive = true;
    const boot = async () => {
      try {
        const info = await api.initVault();
        if (!alive) return;
        apply(info);
        setFatal(null);
        try {
          const imported = await api.startupImport();
          if (alive && imported) {
            apply(imported.vault);
            setSelection({ kind: "deck", id: imported.deckId });
            setVersion((value) => value + 1);
            setNotice("Колода импортирована из файла");
          }
        } catch (err) {
          if (alive) setNotice(String(err));
        }
      } catch (err) {
        if (alive) setFatal(String(err));
      } finally {
        if (alive) setLoading(false);
      }
    };
    void boot();
    return () => {
      alive = false;
    };
  }, [apply]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 2600);
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
      setReviewTarget(null);
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
      const result = await api.createDeck(name);
      apply(result.vault);
      setSelection({ kind: "deck", id: result.deckId });
      setReviewTarget(null);
    },
    [apply],
  );

  const createGroup = useCallback(
    async (name: string) => {
      const result = await api.createGroup(name);
      apply(result.vault);
      setSelection({ kind: "group", id: result.groupId });
      setReviewTarget(null);
    },
    [apply],
  );

  const moveDeck = useCallback(
    async (deckId: string, targetGroup: string) => {
      try {
        const result = await api.moveDeck(deckId, targetGroup);
        apply(result.vault);
        setSelection((prev) =>
          prev?.kind === "deck" && prev.id === deckId
            ? { kind: "deck", id: result.deckId }
            : prev,
        );
        setNotice("Колода перемещена");
      } catch (err) {
        setNotice(String(err));
      }
    },
    [apply],
  );

  const deleteDeck = useCallback(
    async (deck: Deck) => {
      const info = await api.deleteDeck(deck.id);
      apply(info);
      setReviewTarget(null);
      setNotice("Колода удалена");
    },
    [apply],
  );

  const deleteGroup = useCallback(
    async (groupId: string) => {
      const info = await api.deleteGroup(groupId);
      apply(info);
      setReviewTarget(null);
      setNotice("Папка удалена");
    },
    [apply],
  );

  const startReview = useCallback<StartReview>((scope, title, mode, total) => {
    setReviewTarget({ scope, title, mode, total });
  }, []);

  const exportDeck = useCallback(async (deck: Deck) => {
    try {
      const safeName =
        deck.name.replace(/[\\/:*?"<>|]+/g, "-").trim() || "deck";
      const destination = await save({
        defaultPath: `${safeName}.crdz`,
        filters: [{ name: "Cardz Deck", extensions: ["crdz"] }],
      });
      if (!destination) return;
      await api.exportDeck(deck.id, destination);
      setNotice("Колода экспортирована");
    } catch (err) {
      setNotice(String(err));
    }
  }, []);

  const importDeck = useCallback(async () => {
    try {
      const source = await open({
        multiple: false,
        filters: [{ name: "Cardz Deck", extensions: ["crdz"] }],
      });
      if (typeof source !== "string") return;
      const result = await api.importDeck(source);
      apply(result.vault);
      setSelection({ kind: "deck", id: result.deckId });
      setReviewTarget(null);
      setVersion((value) => value + 1);
      setNotice("Колода импортирована");
    } catch (err) {
      setNotice(String(err));
    }
  }, [apply]);

  const selectedDeck =
    selection?.kind === "deck"
      ? (vault?.decks.find((deck) => deck.id === selection.id) ?? null)
      : null;
  const selectedGroup =
    selection?.kind === "group"
      ? (vault?.groups.find((group) => group.id === selection.id) ?? null)
      : null;

  const groupDecks = useMemo(() => {
    if (!vault || !selectedGroup) return [];
    const prefix = `${selectedGroup.id}/`;
    return vault.decks.filter(
      (deck) => deck.id === selectedGroup.id || deck.id.startsWith(prefix),
    );
  }, [selectedGroup, vault]);

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

  if (reviewTarget) {
    const scopeKey =
      reviewTarget.scope.kind === "all"
        ? "all"
        : reviewTarget.scope.id;
    return (
      <Review
        key={`${reviewTarget.scope.kind}:${scopeKey}:${reviewTarget.mode}`}
        target={reviewTarget}
        onExit={() => {
          setReviewTarget(null);
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
        selectedDeckId={selection?.kind === "deck" ? selection.id : null}
        selectedGroupId={selection?.kind === "group" ? selection.id : null}
        onSelectDeck={(id) => setSelection({ kind: "deck", id })}
        onSelectGroup={(id) => setSelection({ kind: "group", id })}
        onCreateDeck={async (name) => {
          await createDeck(name);
        }}
        onCreateGroup={createGroup}
        onMoveDeck={moveDeck}
        onDeleteDeck={deleteDeck}
        onDeleteGroup={deleteGroup}
        onStartReview={startReview}
        onExportDeck={(deck) => void exportDeck(deck)}
        onImportDeck={() => void importDeck()}
        onOpenVault={() => void openFolder()}
        onSync={() => void sync()}
        syncing={syncing}
      />

      {selectedGroup ? (
        <GroupView
          key={selectedGroup.id}
          group={selectedGroup}
          decks={groupDecks}
          onSelectDeck={(id) => setSelection({ kind: "deck", id })}
          onStartReview={startReview}
          onDeleteGroup={deleteGroup}
        />
      ) : selectedDeck ? (
        <DeckView
          key={selectedDeck.id}
          deck={selectedDeck}
          version={version}
          onStartReview={startReview}
          onDeleteDeck={deleteDeck}
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
            <button className="link-btn" onClick={() => void openFolder()}>
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
