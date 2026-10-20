import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import {
  ChevronRight,
  Download,
  Folder,
  FolderOpen,
  FolderPlus,
  Infinity as InfinityIcon,
  Layers,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  Upload,
} from "lucide-react";
import type { Deck, Group, StartReview, VaultInfo } from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";

type TreeNode = {
  key: string;
  name: string;
  deck: Deck | null;
  children: TreeNode[];
};

type ScopeCounts = {
  new: number;
  due: number;
  total: number;
};

type CreateState = {
  kind: "deck" | "group";
  parent: string;
};

type MenuState =
  | { type: "deck"; deck: Deck; x: number; y: number }
  | {
      type: "group";
      id: string;
      name: string;
      counts: ScopeCounts;
      x: number;
      y: number;
    };

function prettify(segment: string): string {
  const spaced = segment.replace(/[-_]+/g, " ");
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

function buildTree(decks: Deck[], groups: Group[]): TreeNode[] {
  const roots: TreeNode[] = [];

  const findOrCreate = (list: TreeNode[], key: string, name: string) => {
    let node = list.find((item) => item.key === key);
    if (!node) {
      node = { key, name, deck: null, children: [] };
      list.push(node);
    }
    return node;
  };

  for (const group of groups) {
    const parts = group.id.split("/").filter(Boolean);
    let list = roots;
    parts.forEach((part, index) => {
      const key = parts.slice(0, index + 1).join("/");
      const isLeaf = index === parts.length - 1;
      const node = findOrCreate(
        list,
        key,
        isLeaf ? group.name : prettify(part),
      );
      if (isLeaf) node.name = group.name;
      list = node.children;
    });
  }

  for (const deck of decks) {
    const parts = deck.id.split("/").filter(Boolean);
    let list = roots;
    parts.forEach((part, index) => {
      const key = parts.slice(0, index + 1).join("/");
      const isLeaf = index === parts.length - 1;
      const node = findOrCreate(list, key, isLeaf ? deck.name : prettify(part));
      if (isLeaf) node.deck = deck;
      list = node.children;
    });
  }

  const sortRec = (list: TreeNode[]) => {
    list.sort((a, b) => {
      const aGroup = a.children.length > 0 || !a.deck;
      const bGroup = b.children.length > 0 || !b.deck;
      if (aGroup !== bGroup) return aGroup ? -1 : 1;
      return a.name.localeCompare(b.name, "ru");
    });
    for (const node of list) sortRec(node.children);
  };
  sortRec(roots);
  return roots;
}

function aggregateCounts(node: TreeNode): ScopeCounts {
  const counts: ScopeCounts = node.deck
    ? {
        new: node.deck.counts.new,
        due: node.deck.counts.due,
        total: node.deck.counts.total,
      }
    : { new: 0, due: 0, total: 0 };
  for (const child of node.children) {
    const childCounts = aggregateCounts(child);
    counts.new += childCounts.new;
    counts.due += childCounts.due;
    counts.total += childCounts.total;
  }
  return counts;
}

function dropTargetAt(x: number, y: number): string | null {
  const element = document.elementFromPoint(x, y);
  if (!element) return null;
  if (element.closest("[data-drop-root]")) return "";
  const group = element.closest("[data-drop-group]") as HTMLElement | null;
  return group?.dataset.dropGroup ?? null;
}

export function Sidebar({
  vault,
  selectedDeckId,
  selectedGroupId,
  onSelectDeck,
  onSelectGroup,
  onCreateDeck,
  onCreateGroup,
  onMoveDeck,
  onDeleteDeck,
  onDeleteGroup,
  onStartReview,
  onExportDeck,
  onImportDeck,
  onOpenVault,
  onSync,
  syncing,
}: {
  vault: VaultInfo;
  selectedDeckId: string | null;
  selectedGroupId: string | null;
  onSelectDeck: (id: string) => void;
  onSelectGroup: (id: string) => void;
  onCreateDeck: (name: string) => Promise<void>;
  onCreateGroup: (name: string) => Promise<void>;
  onMoveDeck: (deckId: string, targetGroup: string) => Promise<void>;
  onDeleteDeck: (deck: Deck) => Promise<void>;
  onDeleteGroup: (groupId: string) => Promise<void>;
  onStartReview: StartReview;
  onExportDeck: (deck: Deck) => void;
  onImportDeck: () => void;
  onOpenVault: () => void;
  onSync: () => void;
  syncing: boolean;
}) {
  const tree = useMemo(
    () => buildTree(vault.decks, vault.groups),
    [vault.decks, vault.groups],
  );
  const globalCounts = useMemo(() => {
    let available = 0;
    let total = 0;
    for (const deck of vault.decks) {
      available += deck.counts.new + deck.counts.due;
      total += deck.counts.total;
    }
    return { available, total };
  }, [vault.decks]);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => {
    try {
      const saved = localStorage.getItem("cardz.collapsedGroups");
      return new Set<string>(saved ? JSON.parse(saved) : []);
    } catch {
      return new Set<string>();
    }
  });
  const [creating, setCreating] = useState<CreateState | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Deck | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [deleteFolder, setDeleteFolder] = useState<{
    id: string;
    name: string;
  } | null>(null);
  const [deletingFolder, setDeletingFolder] = useState(false);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [dragDeck, setDragDeck] = useState<Deck | null>(null);
  const [dragPoint, setDragPoint] = useState<{ x: number; y: number } | null>(
    null,
  );
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const dragRef = useRef<{
    deck: Deck;
    startX: number;
    startY: number;
    moved: boolean;
  } | null>(null);
  const suppressClickRef = useRef(false);

  useEffect(() => {
    localStorage.setItem("cardz.collapsedGroups", JSON.stringify([...collapsed]));
  }, [collapsed]);

  useEffect(() => {
    if (!dragDeck) return;
    document.body.classList.add("deck-dragging");
    return () => document.body.classList.remove("deck-dragging");
  }, [dragDeck]);

  useEffect(() => {
    if (!menu) return;
    const closeOnPointer = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest(".context-menu")) return;
      setMenu(null);
    };
    const closeOnKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(null);
    };
    const closeOnBlur = () => setMenu(null);
    window.addEventListener("pointerdown", closeOnPointer);
    window.addEventListener("keydown", closeOnKey);
    window.addEventListener("blur", closeOnBlur);
    return () => {
      window.removeEventListener("pointerdown", closeOnPointer);
      window.removeEventListener("keydown", closeOnKey);
      window.removeEventListener("blur", closeOnBlur);
    };
  }, [menu]);

  const toggleGroup = (key: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const startCreate = (kind: "deck" | "group", parent: string) => {
    setMenu(null);
    setCreating({ kind, parent });
    setName("");
    setFormError(null);
    if (parent && collapsed.has(parent)) {
      setCollapsed((prev) => {
        const next = new Set(prev);
        next.delete(parent);
        return next;
      });
    }
  };

  const submit = async (state: CreateState) => {
    const value = name.trim();
    if (!value || busy) return;
    const path = state.parent ? `${state.parent}/${value}` : value;
    setBusy(true);
    setFormError(null);
    try {
      if (state.kind === "deck") await onCreateDeck(path);
      else await onCreateGroup(path);
      setName("");
      setCreating(null);
    } catch (err) {
      setFormError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const openDeckMenu = (event: ReactMouseEvent, deck: Deck) => {
    event.preventDefault();
    event.stopPropagation();
    const width = 250;
    const height = 270;
    setMenu({
      type: "deck",
      deck,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
    });
  };

  const openGroupMenu = (
    event: ReactMouseEvent,
    node: TreeNode,
    counts: ScopeCounts,
  ) => {
    event.preventDefault();
    event.stopPropagation();
    const width = 250;
    const height = 310;
    setMenu({
      type: "group",
      id: node.key,
      name: node.name,
      counts,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
    });
  };

  const finishDrag = () => {
    dragRef.current = null;
    setDragDeck(null);
    setDragPoint(null);
    setDropTarget(null);
  };

  const moveDeckTo = (deck: Deck, target: string) => {
    if (target === deck.id || target.startsWith(`${deck.id}/`)) return;
    const currentParent = deck.id.split("/").slice(0, -1).join("/");
    if (currentParent === target) return;
    if (target && collapsed.has(target)) {
      setCollapsed((prev) => {
        const next = new Set(prev);
        next.delete(target);
        return next;
      });
    }
    void onMoveDeck(deck.id, target);
  };

  const beginDrag = (
    event: ReactPointerEvent<HTMLButtonElement>,
    deck: Deck,
  ) => {
    if (event.button !== 0) return;
    setMenu(null);
    dragRef.current = {
      deck,
      startX: event.clientX,
      startY: event.clientY,
      moved: false,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const moveDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const current = dragRef.current;
    if (!current) return;
    if (!current.moved) {
      const distance = Math.hypot(
        event.clientX - current.startX,
        event.clientY - current.startY,
      );
      if (distance < 6) return;
      current.moved = true;
      setDragDeck(current.deck);
    }
    event.preventDefault();
    setDragPoint({ x: event.clientX, y: event.clientY });
    setDropTarget(dropTargetAt(event.clientX, event.clientY));
  };

  const endDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const current = dragRef.current;
    if (!current) return;
    const target = dropTargetAt(event.clientX, event.clientY);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (current.moved) suppressClickRef.current = true;
    finishDrag();
    if (!current.moved || target === null) return;
    moveDeckTo(current.deck, target);
  };

  const renderForm = (state: CreateState) => (
    <>
      <form
        className="new-deck nested"
        onSubmit={(event) => {
          event.preventDefault();
          void submit(state);
        }}
      >
        <input
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={
            state.kind === "group"
              ? state.parent
                ? "Подгруппа"
                : "Название группы"
              : state.parent
                ? "Колода в группе"
                : "Название колоды"
          }
          onKeyDown={(event) => {
            if (event.key === "Escape") setCreating(null);
          }}
        />
        <button type="submit" className="mini-primary" disabled={busy}>
          {busy ? <Loader2 size={14} className="spin" /> : "Создать"}
        </button>
      </form>
      {formError && <div className="side-form-error">{formError}</div>}
    </>
  );

  const renderNodes = (nodes: TreeNode[], depth: number): ReactNode =>
    nodes.map((node) => {
      const hasChildren = node.children.length > 0;
      const counts = aggregateCounts(node);
      const available = counts.new + counts.due;

      if (!hasChildren && node.deck) {
        const deck = node.deck;
        const active = deck.id === selectedDeckId;
        return (
          <button
            key={node.key}
            className={`deck-item ${active ? "active" : ""} ${
              dragDeck?.id === deck.id ? "dragging" : ""
            }`}
            style={{ paddingLeft: 10 + depth * 13 }}
            onPointerDown={(event) => beginDrag(event, deck)}
            onPointerMove={moveDrag}
            onPointerUp={endDrag}
            onPointerCancel={finishDrag}
            onLostPointerCapture={() => {
              if (dragRef.current) finishDrag();
            }}
            onClick={() => {
              if (suppressClickRef.current) {
                suppressClickRef.current = false;
                return;
              }
              onSelectDeck(deck.id);
            }}
            onContextMenu={(event) => openDeckMenu(event, deck)}
          >
            <span
              className="deck-dot"
              style={{ background: `hsl(${hashHue(deck.id)} 70% 62%)` }}
            />
            <span className="deck-name">{deck.name}</span>
            {available > 0 && (
              <span className="deck-badge">{available}</span>
            )}
          </button>
        );
      }

      const isCollapsed = collapsed.has(node.key);
      const active = node.key === selectedGroupId;
      return (
        <div key={node.key} className="deck-group">
          <div
            className={`group-row ${
              dropTarget === node.key ? "drop-over" : ""
            }`}
            style={{ paddingLeft: 2 + depth * 13 }}
            data-drop-group={node.key}
            onContextMenu={(event) => openGroupMenu(event, node, counts)}
          >
            <button
              className="group-chevron-btn"
              onClick={() => toggleGroup(node.key)}
              title={isCollapsed ? "Развернуть" : "Свернуть"}
            >
              <ChevronRight
                size={13}
                className={`group-chevron ${isCollapsed ? "" : "open"}`}
              />
            </button>
            <button
              className={`group-select ${active ? "active" : ""}`}
              onClick={() => onSelectGroup(node.key)}
            >
              {isCollapsed ? <Folder size={14} /> : <FolderOpen size={14} />}
              <span className="group-name">{node.name}</span>
              {available > 0 && <span className="deck-badge">{available}</span>}
            </button>
            <span className="group-actions">

              <button
                className="icon-btn small group-add"
                title="Новая подгруппа"
                onClick={(event) => {
                  event.stopPropagation();
                  startCreate("group", node.key);
                }}
              >
                <FolderPlus size={13} />
              </button>
              <button
                className="icon-btn small group-add"
                title="Новая колода"
                onClick={(event) => {
                  event.stopPropagation();
                  startCreate("deck", node.key);
                }}
              >
                <Plus size={13} />
              </button>
            </span>
          </div>
          {!isCollapsed && (
            <div className="group-children">
              {creating?.parent === node.key && renderForm(creating)}
              {renderNodes(node.children, depth + 1)}
            </div>
          )}
        </div>
      );
    });

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="logo">c</div>
        <div className="brand-text">
          <span className="brand-name">cardz</span>
          <span className="brand-sub">local-first</span>
        </div>
      </div>

      {vault.decks.length > 0 && (
        <div className="review-all">
          <button
            className="review-all-main"
            disabled={globalCounts.available === 0}
            onClick={() =>
              onStartReview(
                { kind: "all" },
                "Все карточки",
                "scheduled",
                globalCounts.available,
              )
            }
            title="Новые и просроченные карточки из всех колод"
          >
            <Play size={15} />
            <span className="review-all-label">Повторить всё</span>
            <span className="review-all-count">{globalCounts.available}</span>
          </button>
          <button
            className="review-all-sub"
            disabled={globalCounts.total === 0}
            onClick={() =>
              onStartReview(
                { kind: "all" },
                "Все карточки",
                "all",
                globalCounts.total,
              )
            }
            title="Все карточки, включая запланированные; ответы обновят интервалы"
          >
            <InfinityIcon size={14} />
            <span>Все карточки</span>
            <span className="review-all-count">{globalCounts.total}</span>
          </button>
        </div>
      )}

      <div className="side-section">
        <div className="side-label">
          <span>Колоды</span>
          <span className="side-label-actions">
            <button
              className="icon-btn small"
              onClick={() =>
                creating?.kind === "group" && creating.parent === ""
                  ? setCreating(null)
                  : startCreate("group", "")
              }
              title="Новая группа"
            >
              <FolderPlus size={15} />
            </button>
            <button
              className="icon-btn small"
              onClick={() =>
                creating?.kind === "deck" && creating.parent === ""
                  ? setCreating(null)
                  : startCreate("deck", "")
              }
              title="Новая колода"
            >
              <Plus size={15} />
            </button>
          </span>
        </div>

        {creating?.parent === "" && renderForm(creating)}

        <nav className="deck-list">
          {dragDeck && (
            <div
              className={`root-drop ${
                dropTarget === "__root__" ? "over" : ""
              }`}
              data-drop-root="true"
            >
              Переместить в корень
            </div>
          )}
          {renderNodes(tree, 0)}
          {vault.decks.length === 0 &&
            vault.groups.length === 0 &&
            !creating && <div className="side-empty">Пока нет колод</div>}
        </nav>
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
            onClick={onImportDeck}
            title="Импортировать колоду (.crdz)"
          >
            <Upload size={15} />
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

      {dragDeck && dragPoint && (
        <div
          className="drag-ghost"
          style={{ left: dragPoint.x + 14, top: dragPoint.y + 14 }}
        >
          <span
            className="drag-ghost-dot"
            style={{ background: `hsl(${hashHue(dragDeck.id)} 70% 62%)` }}
          />
          <span className="drag-ghost-name">{dragDeck.name}</span>
        </div>
      )}

      {menu && (
        <div
          className="context-menu"
          style={{ left: menu.x, top: menu.y }}
          onContextMenu={(event) => event.preventDefault()}
        >
          {menu.type === "deck" ? (
            <>
              <button
                className="context-item"
                onClick={() => {
                  onSelectDeck(menu.deck.id);
                  setMenu(null);
                }}
              >
                <FolderOpen size={14} /> Открыть колоду
              </button>
              <button
                className="context-item"
                disabled={menu.deck.counts.new + menu.deck.counts.due === 0}
                onClick={() => {
                  onStartReview(
                    { kind: "deck", id: menu.deck.id },
                    menu.deck.name,
                    "scheduled",
                    menu.deck.counts.new + menu.deck.counts.due,
                  );
                  setMenu(null);
                }}
              >
                <Play size={14} /> По расписанию ·{" "}
                {menu.deck.counts.new + menu.deck.counts.due}
              </button>
              <button
                className="context-item"
                disabled={menu.deck.counts.total === 0}
                onClick={() => {
                  onStartReview(
                    { kind: "deck", id: menu.deck.id },
                    menu.deck.name,
                    "all",
                    menu.deck.counts.total,
                  );
                  setMenu(null);
                }}
              >
                <InfinityIcon size={14} /> Все карточки ·{" "}
                {menu.deck.counts.total}
              </button>
              <div className="context-sep" />
              <button
                className="context-item"
                onClick={() => {
                  onExportDeck(menu.deck);
                  setMenu(null);
                }}
              >
                <Download size={14} /> Экспортировать .crdz
              </button>
              <div className="context-sep" />
              <button
                className="context-item danger"
                onClick={() => {
                  setDeleteError(null);
                  setDeleteTarget(menu.deck);
                  setMenu(null);
                }}
              >
                <Trash2 size={14} /> Удалить колоду…
              </button>
            </>
          ) : (
            <>
              <button
                className="context-item"
                onClick={() => {
                  onSelectGroup(menu.id);
                  setMenu(null);
                }}
              >
                <FolderOpen size={14} /> Открыть папку
              </button>
              <button
                className="context-item"
                disabled={menu.counts.new + menu.counts.due === 0}
                onClick={() => {
                  onStartReview(
                    { kind: "group", id: menu.id },
                    menu.name,
                    "scheduled",
                    menu.counts.new + menu.counts.due,
                  );
                  setMenu(null);
                }}
              >
                <Play size={14} /> По расписанию ·{" "}
                {menu.counts.new + menu.counts.due}
              </button>
              <button
                className="context-item"
                disabled={menu.counts.total === 0}
                onClick={() => {
                  onStartReview(
                    { kind: "group", id: menu.id },
                    menu.name,
                    "all",
                    menu.counts.total,
                  );
                  setMenu(null);
                }}
              >
                <InfinityIcon size={14} /> Все карточки · {menu.counts.total}
              </button>
              <div className="context-sep" />
              <button
                className="context-item"
                onClick={() => startCreate("deck", menu.id)}
              >
                <Plus size={14} /> Новая колода
              </button>
              <button
                className="context-item"
                onClick={() => startCreate("group", menu.id)}
              >
                <FolderPlus size={14} /> Новая подгруппа
              </button>
              <div className="context-sep" />
              <button
                className="context-item danger"
                onClick={() => {
                  setFolderError(null);
                  setDeleteFolder({ id: menu.id, name: menu.name });
                  setMenu(null);
                }}
              >
                <Trash2 size={14} /> Удалить папку…
              </button>
            </>
          )}
        </div>
      )}
      {deleteTarget && (
        <ConfirmDialog
          title="Удалить колоду?"
          message={
            <>
              Папка <strong>{deleteTarget.name}</strong> со всеми заметками,
              медиа и вложенными колодами будет удалена безвозвратно.
            </>
          }
          busy={deleting}
          error={deleteError}
          onCancel={() => {
            if (!deleting) setDeleteTarget(null);
          }}
          onConfirm={async () => {
            if (deleting) return;
            setDeleting(true);
            setDeleteError(null);
            try {
              await onDeleteDeck(deleteTarget);
              setDeleteTarget(null);
            } catch (err) {
              setDeleteError(String(err));
            } finally {
              setDeleting(false);
            }
          }}
        />
      )}
      {deleteFolder && (
        <ConfirmDialog
          title="Удалить папку?"
          message={
            <>
              Папка <strong>{deleteFolder.name}</strong> и все вложенные колоды
              со всеми карточками и медиа будут удалены безвозвратно.
            </>
          }
          busy={deletingFolder}
          error={folderError}
          onCancel={() => {
            if (!deletingFolder) setDeleteFolder(null);
          }}
          onConfirm={async () => {
            if (deletingFolder) return;
            setDeletingFolder(true);
            setFolderError(null);
            try {
              await onDeleteGroup(deleteFolder.id);
              setDeleteFolder(null);
            } catch (err) {
              setFolderError(String(err));
            } finally {
              setDeletingFolder(false);
            }
          }}
        />
      )}
    </aside>
  );
}

function hashHue(id: string): number {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) {
    hash = (hash * 31 + id.charCodeAt(i)) % 360;
  }
  return hash;
}
