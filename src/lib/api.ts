import { invoke } from "@tauri-apps/api/core";
import type {
  AnswerResult,
  CreateDeckResult,
  CreateGroupResult,
  NoteDetail,
  NoteSummary,
  ReviewCard,
  ReviewScope,
  SaveNoteInput,
  Template,
  VaultInfo,
} from "./types";

export const api = {
  initVault: () => invoke<VaultInfo>("init_vault"),
  openVault: (path: string) => invoke<VaultInfo>("open_vault", { path }),
  vaultInfo: () => invoke<VaultInfo>("vault_info"),
  syncVault: () => invoke<VaultInfo>("sync_vault"),
  createDeck: (name: string, description?: string) =>
    invoke<CreateDeckResult>("create_deck", {
      name,
      description: description ?? null,
    }),
  createGroup: (name: string) =>
    invoke<CreateGroupResult>("create_group", { name }),
  deleteDeck: (deckId: string) =>
    invoke<VaultInfo>("delete_deck", { deckId }),
  deleteGroup: (groupId: string) =>
    invoke<VaultInfo>("delete_group", { groupId }),
  moveDeck: (deckId: string, targetGroup: string) =>
    invoke<CreateDeckResult>("move_deck", { deckId, targetGroup }),
  exportDeck: (deckId: string, destPath: string) =>
    invoke<void>("export_deck", { deckId, destPath }),
  importDeck: (sourcePath: string) =>
    invoke<CreateDeckResult>("import_deck", { sourcePath }),
  startupImport: () =>
    invoke<CreateDeckResult | null>("startup_import"),
  listTemplates: (deckId: string) => invoke<Template[]>("list_templates", { deckId }),
  listNotes: (deckId: string, query?: string) =>
    invoke<NoteSummary[]>("list_notes", { deckId, query: query ?? null }),
  getNote: (deckId: string, noteId: string) =>
    invoke<NoteDetail>("get_note", { deckId, noteId }),
  saveNote: (input: SaveNoteInput) => invoke<NoteDetail>("save_note", { input }),
  deleteNote: (deckId: string, noteId: string) =>
    invoke<void>("delete_note", { deckId, noteId }),
  importMedia: (deckId: string, sourcePath: string) =>
    invoke<string>("import_media", { deckId, sourcePath }),
  readMedia: (deckId: string, path: string) =>
    invoke<string>("read_media", { deckId, path }),
  nextCard: (scope: ReviewScope, includeFuture: boolean, exclude: string[]) =>
    invoke<ReviewCard | null>("next_card", {
      scope,
      includeFuture,
      exclude,
    }),
  answerCard: (cardId: string, rating: number) =>
    invoke<AnswerResult>("answer_card", { cardId, rating }),
};
