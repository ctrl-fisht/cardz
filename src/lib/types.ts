export type Counts = {
  new: number;
  due: number;
  learning: number;
  total: number;
  reviewedToday: number;
};

export type Deck = {
  id: string;
  name: string;
  description: string;
  path: string;
  counts: Counts;
};

export type VaultInfo = {
  root: string;
  name: string;
  decks: Deck[];
  groups: Group[];
};

export type Group = {
  id: string;
  name: string;
  path: string;
};

export type CreateDeckResult = {
  vault: VaultInfo;
  deckId: string;
};

export type CreateGroupResult = {
  vault: VaultInfo;
  groupId: string;
};

export type NoteSummary = {
  id: string;
  deckId: string;
  noteType: string;
  tags: string[];
  preview: string;
  modified: number;
};

export type NoteDetail = {
  id: string;
  deckId: string;
  path: string;
  noteType: string;
  tags: string[];
  fields: Record<string, string>;
  modified: number;
};

export type CardTemplate = {
  name: string;
  front: string;
  back: string;
};

export type ReviewMode = "scheduled" | "all";

export type ReviewScope =
  | { kind: "deck"; id: string }
  | { kind: "group"; id: string }
  | { kind: "all" };

export type ReviewTarget = {
  scope: ReviewScope;
  title: string;
  mode: ReviewMode;
  total: number;
};

export type StartReview = (
  scope: ReviewScope,
  title: string,
  mode: ReviewMode,
  total: number,
) => void;

export type SyncStatus = {
  configured: boolean;
  gitAvailable: boolean;
  repo: boolean;
  remote: string | null;
  branch: string;
  autoSync: boolean;
  dirty: boolean;
  ahead: number;
  behind: number;
  conflicts: string[];
  lastSync: number | null;
  error: string | null;
  syncing: boolean;
};

export type Template = {
  id: string;
  name: string;
  fields: string[];
  labels: Record<string, string>;
  cards: CardTemplate[];
};

export type ReviewCard = {
  cardId: string;
  noteId: string;
  deckId: string;
  deckName: string;
  deckPath: string;
  fields: Record<string, string>;
  tags: string[];
  cardName: string;
  front: string;
  back: string;
  state: number;
  intervals: Record<string, string>;
  isNew: boolean;
};

export type AnswerResult = {
  due: number;
  label: string;
  state: number;
};

export type SaveNoteInput = {
  deckId: string;
  noteId?: string;
  noteType: string;
  tags: string[];
  fields: Record<string, string>;
};

export type AiSettings = {
  cliPath: string | null;
  model: string;
  minutes: number;
  style: "friendly" | "strict" | "quick";
};

export type AiStatus = {
  available: boolean;
  version: string | null;
  cliPath: string | null;
  error: string | null;
};

export type AiMessage = {
  role: "user" | "assistant";
  content: string;
  at: number;
};

export type AiGap = {
  topic: string;
  detail: string;
  severity: "high" | "medium" | "low";
};

export type AiVerdict = {
  summary: string;
  level: "weak" | "ok" | "strong";
  strengths: string[];
  gaps: AiGap[];
  toReview: string[];
  nextQuestions: string[];
};

export type AiCardProposal = {
  front: string;
  back: string;
  tags: string[];
  topic: string;
  duplicate?: boolean;
};

export type InterviewStartResult = {
  interviewId: string;
  sessionId: string;
  reply: string;
  materialTotal: number;
  materialIncluded: number;
};

export type VerdictResult = {
  reply: string | null;
  verdict: AiVerdict;
};

export type CommitResult = {
  created: number;
  skipped: number;
  deckId: string;
};

export type InterviewRecord = {
  id: string;
  title: string;
  scope: ReviewScope;
  model: string;
  createdAt: number;
  updatedAt: number;
  opencodeSessionId: string | null;
  messages: AiMessage[];
  verdict: AiVerdict | null;
  cards: AiCardProposal[];
  cardsCreated: number;
  status: string;
  durationSec: number;
  materialTotal: number;
  materialIncluded: number;
};

export type InterviewSummary = {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  status: string;
  level: string | null;
  messageCount: number;
  cardsCreated: number;
  model: string;
  durationSec: number;
};

export type ResumeResult = {
  record: InterviewRecord;
  sessionId: string | null;
  readOnly: boolean;
  notice: string | null;
};

export type AiEvent = {
  type: string;
  properties: Record<string, unknown>;
};

export type StartInterview = (scope: ReviewScope, title: string) => void;
