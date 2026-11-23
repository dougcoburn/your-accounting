export type AccountType = "asset" | "liability" | "equity" | "income" | "expense";

export type AccountKind =
  | "cash"
  | "card"
  | "investment"
  | "receivable"
  | "payable"
  | "equity"
  | "category";

export type Side = "debit" | "credit";

export type SourceKind = "fixture" | "env";

export interface Entity {
  id: string;
  name: string;
}

export interface Party {
  id: string;
  name: string;
}

export interface LedgerAccount {
  id: string;
  name: string;
  entityId: string;
  accountType: AccountType;
  kind: AccountKind;
  parentId: string | null;
  leaf: boolean;
}

export interface Observation {
  id: string;
  occurredOn: string;
  postedOn: string | null;
  merchant: string;
  category: string;
  accountName: string;
  originalStatement: string;
  notes: string;
  amountMinor: number;
  tags: string[];
  owner: string;
  reviewed: string;
  reviewedAt: string;
  reviewedBy: string;
  pending: string;
}

export interface Posting {
  id: string;
  accountId: string;
  entityId: string;
  side: Side;
  amountMinor: number;
  currency: "USD";
}

export interface JournalEvent {
  id: string;
  recordedAt: string;
  effectiveOn: string;
  memo: string;
  observationIds: string[];
  reversesId: string | null;
  supersededBy: string | null;
  reason: string | null;
  postings: Posting[];
}

export interface Unposted {
  observationId: string;
  reason: string;
}

export interface StatementHint {
  accountName: string;
  cutoff: string;
  endingMinor: number;
  note: string;
}

export interface DemonstrationStep {
  fromAmountMinor: number;
  amountMinor: number;
  categoryId: string;
  reason: string;
}

export interface Demonstration {
  observationId: string;
  steps: DemonstrationStep[];
}

export interface IdSource {
  event(): string;
  posting(): string;
  stamp(): string;
}

export interface Ledger {
  source: SourceKind;
  currency: "USD";
  entities: Entity[];
  parties: Party[];
  accounts: LedgerAccount[];
  observations: Observation[];
  events: JournalEvent[];
  unposted: Unposted[];
  statements: StatementHint[];
  defaultCutoff: string;
  demonstration: Demonstration | null;
  ids: IdSource;
}

export interface CorrectionInput {
  eventId: string;
  amountMinor: number;
  categoryAccountId: string | null;
  reason: string;
}
