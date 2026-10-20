import { invoke } from "@tauri-apps/api/core";
import type {
  AiCardProposal,
  AiSettings,
  AiStatus,
  AnswerResult,
  CommitResult,
  CreateDeckResult,
  CreateGroupResult,
  InterviewRecord,
  InterviewStartResult,
  InterviewSummary,
  NoteDetail,
  NoteSummary,
  ResumeResult,
  ReviewCard,
  ReviewScope,
  SaveNoteInput,
  SyncStatus,
  Template,
  VaultInfo,
  VerdictResult,
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
  syncStatus: () => invoke<SyncStatus>("sync_status"),
  syncConnect: (remote: string, branch: string, autoSync: boolean) =>
    invoke<SyncStatus>("sync_connect", { remote, branch, autoSync }),
  syncNow: () => invoke<SyncStatus>("sync_now"),
  syncPull: () => invoke<SyncStatus>("sync_pull"),
  syncDisconnect: () => invoke<SyncStatus>("sync_disconnect"),
  syncResolve: (path: string, side: "local" | "remote") =>
    invoke<SyncStatus>("sync_resolve", { path, side }),
  syncAbort: () => invoke<SyncStatus>("sync_abort"),
  aiSettingsGet: () => invoke<AiSettings>("ai_settings_get"),
  aiSettingsSave: (settings: AiSettings) =>
    invoke<AiSettings>("ai_settings_save", { settings }),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  aiModels: () => invoke<string[]>("ai_models"),
  aiInterviewStart: (scope: ReviewScope, title: string) =>
    invoke<InterviewStartResult>("ai_interview_start", { scope, title }),
  aiInterviewSend: (sessionId: string, message: string) =>
    invoke<string>("ai_interview_send", { sessionId, message }),
  aiInterviewVerdict: (sessionId: string, objection?: string | null) =>
    invoke<VerdictResult>("ai_interview_verdict", {
      sessionId,
      objection: objection ?? null,
    }),
  aiInterviewCards: (sessionId: string) =>
    invoke<AiCardProposal[]>("ai_interview_cards", { sessionId }),
  aiInterviewCommit: (
    interviewId: string,
    deckId: string,
    cards: AiCardProposal[],
  ) =>
    invoke<CommitResult>("ai_interview_commit", {
      interviewId,
      deckId,
      cards,
    }),
  aiAssistantGenerate: (
    deckId: string,
    request: string,
    existing: string[],
    count?: number,
  ) =>
    invoke<AiCardProposal[]>("ai_assistant_generate", {
      deckId,
      request,
      existing,
      count: count ?? null,
    }),
  aiAssistantCommit: (deckId: string, cards: AiCardProposal[]) =>
    invoke<CommitResult>("ai_assistant_commit", { deckId, cards }),
  aiInterviewAbort: (sessionId: string) =>
    invoke<void>("ai_interview_abort", { sessionId }),
  aiInterviewResume: (interviewId: string) =>
    invoke<ResumeResult>("ai_interview_resume", { interviewId }),
  aiHistoryList: () => invoke<InterviewSummary[]>("ai_history_list"),
  aiHistoryGet: (id: string) => invoke<InterviewRecord>("ai_history_get", { id }),
  aiHistoryDelete: (id: string) => invoke<void>("ai_history_delete", { id }),
  aiHistoryClear: () => invoke<void>("ai_history_clear"),
};
