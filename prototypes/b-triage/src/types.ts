/** Review state of a register / worklist line. See repo GLOSSARY.md. */
export type ReviewState =
  | "unreviewed"
  | "interpreted"
  | "reconciled"
  | "voided"
  | "open";

export type Origin = "observed" | "expected";

export type RecipeId =
  | "purchase"
  | "refund"
  | "perk-credit"
  | "card-payment"
  | "transfer"
  | "paycheck";

export type Observation = {
  id: string;
  date: string;
  merchant: string;
  category: string;
  account: string;
  originalStatement: string;
  notes: string;
  /** Signed minor units (cents). Negative is an outflow. */
  amountMinor: number;
  tags: string;
  owner: string;
  reviewed: boolean;
  reviewedAt: string;
  reviewedBy: string;
  pending: boolean;
};

export type SettlementParticipant = {
  role: string;
  label: string;
  origin: Origin;
  filled: boolean;
  amountMinor: number;
  account: string;
  observationId?: string;
};

export type SettlementGroupSuggestion = {
  title: string;
  recipeId: RecipeId;
  participants: SettlementParticipant[];
  complete: boolean;
  residualMinor: number;
  note?: string;
};

export type Treatment = {
  recipeId: RecipeId;
  recipeLabel: string;
  /** Leaf category (income/expense ledger account) when the recipe posts to one. */
  category: string;
  summary: string;
  settlementGroup?: SettlementGroupSuggestion;
};

export type JournalPosting = {
  account: string;
  debitMinor: number;
  creditMinor: number;
};

export type JournalEntry = {
  id: string;
  date: string;
  recipeId: RecipeId;
  recipeLabel: string;
  memo: string;
  settlementGroupId: string;
  postings: JournalPosting[];
  sourceKeys: string[];
};

export type ExpectedMovement = {
  key: string;
  label: string;
  account: string;
  amountMinor: number;
  date: string;
  relatedObservationId: string;
  recipeId: RecipeId;
};

export type Line = {
  key: string;
  origin: Origin;
  state: ReviewState;
  observation?: Observation;
  expected?: ExpectedMovement;
  /** Recipe applied when the line left unreviewed (if any). */
  appliedRecipe?: string;
};

export type Books = {
  lines: Line[];
  entries: JournalEntry[];
  entrySeq: number;
  expectedSeq: number;
  groupSeq: number;
};

export type Progress = {
  total: number;
  unreviewed: number;
  interpreted: number;
  reconciled: number;
  voided: number;
  open: number;
  /** reconciled + voided + open */
  reached: number;
};
