import { invoke } from "@tauri-apps/api/core";
import type {
  AnswerResult,
  NoteDetail,
  NoteSummary,
  ReviewCard,
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
    invoke<VaultInfo>("create_deck", { name, description: description ?? null }),
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
  nextCard: (deckId: string) => invoke<ReviewCard | null>("next_card", { deckId }),
  answerCard: (cardId: string, rating: number) =>
    invoke<AnswerResult>("answer_card", { cardId, rating }),
};
