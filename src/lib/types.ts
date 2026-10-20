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
