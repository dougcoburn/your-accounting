import { FeedError } from "./errors.ts";
import type {
  AccountKind,
  AccountType,
  Demonstration,
  Entity,
  LedgerAccount,
  Party,
  StatementHint,
} from "./types.ts";

export interface BooksFile {
  marker: string;
  currency: "USD";
  defaultCutoff: string;
  entities: Entity[];
  parties: Party[];
  tagMap: Record<string, string>;
  accounts: Array<{
    id: string;
    name: string;
    entityId: string;
    accountType: AccountType;
    kind: AccountKind;
    openingMinor: number;
    openingOn: string;
  }>;
  categories: Array<{
    id: string;
    name: string;
    entityId: string;
    accountType: "income" | "expense";
    parentId: string | null;
    leaf: boolean;
  }>;
  postedDates: Record<string, string>;
  statements: StatementHint[];
  demonstration: Demonstration;
}

export interface Opening {
  accountId: string;
  amountMinor: number;
  openingOn: string;
}

export interface WorkingChart {
  strict: boolean;
  entities: Entity[];
  parties: Party[];
  accounts: LedgerAccount[];
  openings: Opening[];
  statements: StatementHint[];
  defaultCutoff: string | null;
  demonstration: Demonstration | null;
  describeTag(tag: string): { id: string; name: string; role: "entity" | "party" } | null;
  financialByName(name: string): LedgerAccount | null;
  leafCategory(entityId: string, name: string): LedgerAccount | null;
  accountById(id: string): LedgerAccount | null;
  ensureEquity(entityId: string): LedgerAccount;
  ensureDueFrom(ownerEntityId: string, counterpartyId: string, counterpartyName: string): LedgerAccount;
  ensureDueTo(ownerEntityId: string, counterpartyId: string, counterpartyName: string): LedgerAccount;
  ensureInferredFinancial(name: string): LedgerAccount;
  ensureInferredCategory(entityId: string, name: string): LedgerAccount;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function needString(record: Record<string, unknown>, key: string): string {
  const value = record[key];
  if (typeof value !== "string" || value.trim() === "") {
    throw new FeedError("The fixture chart is missing a name.");
  }
  return value;
}

export function parseBooks(value: unknown): BooksFile {
  if (!isRecord(value)) {
    throw new FeedError("The fixture chart could not be read.");
  }
  if (typeof value.marker !== "string" || !value.marker.includes("# synthetic")) {
    throw new FeedError("The fixture chart is missing its synthetic marker.");
  }
  if (value.currency !== "USD") {
    throw new FeedError("This prototype keeps one currency, USD.");
  }
  if (!Array.isArray(value.entities) || !Array.isArray(value.accounts) || !Array.isArray(value.categories) || !Array.isArray(value.statements)) {
    throw new FeedError("The fixture chart could not be read.");
  }
  return value as BooksFile;
}

function slug(value: string): string {
  const out = value.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");
  return out || "account";
}

function inferFinancial(name: string): { accountType: AccountType; kind: AccountKind } {
  if (/visa|mastercard|amex|discover|credit card|\bcard\b/i.test(name)) {
    return { accountType: "liability", kind: "card" };
  }
  if (/loan|mortgage/i.test(name)) {
    return { accountType: "liability", kind: "card" };
  }
  if (/hsa|401k|ira|brokerage|investment/i.test(name)) {
    return { accountType: "asset", kind: "investment" };
  }
  return { accountType: "asset", kind: "cash" };
}

function inferCategoryType(name: string): "income" | "expense" {
  if (/wage|salary|paycheck|interest|dividend|income|revenue/i.test(name)) {
    return "income";
  }
  return "expense";
}

export function loadChart(books: BooksFile | null): WorkingChart {
  const entities: Entity[] = books
    ? books.entities.map((entity) => ({ ...entity }))
    : [{ id: "ent_household", name: "Household" }];
  const parties: Party[] = books ? books.parties.map((party) => ({ ...party })) : [];
  const accounts: LedgerAccount[] = [];
  const openings: Opening[] = [];
  const tagToId = new Map<string, string>();
  if (books) {
    for (const [tag, id] of Object.entries(books.tagMap)) {
      tagToId.set(tag.toLowerCase(), id);
    }
    for (const account of books.accounts) {
      accounts.push({
        id: account.id,
        name: account.name,
        entityId: account.entityId,
        accountType: account.accountType,
        kind: account.kind,
        parentId: null,
        leaf: true,
      });
      if (account.openingMinor !== 0) {
        openings.push({
          accountId: account.id,
          amountMinor: account.openingMinor,
          openingOn: account.openingOn,
        });
      }
    }
    for (const category of books.categories) {
      accounts.push({
        id: category.id,
        name: category.name,
        entityId: category.entityId,
        accountType: category.accountType,
        kind: "category",
        parentId: category.parentId,
        leaf: category.leaf,
      });
    }
  }

  const byId = () => new Map(accounts.map((account) => [account.id, account]));

  function addAccount(account: LedgerAccount): LedgerAccount {
    if (byId().has(account.id)) {
      return byId().get(account.id)!;
    }
    accounts.push(account);
    return account;
  }

  function uniqueId(base: string): string {
    let id = base;
    let n = 2;
    const taken = () => accounts.some((account) => account.id === id);
    while (taken()) {
      id = `${base}_${n}`;
      n += 1;
    }
    return id;
  }

  const chart: WorkingChart = {
    strict: books !== null,
    entities,
    parties,
    accounts,
    openings,
    statements: books ? books.statements : [],
    defaultCutoff: books ? books.defaultCutoff : null,
    demonstration: books ? books.demonstration : null,
    describeTag(tag: string) {
      const key = tag.toLowerCase();
      let id = tagToId.get(key) ?? null;
      if (!id && !books) {
        const name = key.replace(/^e-/, "").replace(/[-_]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
        id = uniqueId(`party_${slug(key)}`);
        parties.push({ id, name: name || "Party" });
        tagToId.set(key, id);
      }
      if (!id) {
        return null;
      }
      const entity = entities.find((item) => item.id === id);
      if (entity) {
        return { id: entity.id, name: entity.name, role: "entity" };
      }
      const party = parties.find((item) => item.id === id);
      if (party) {
        return { id: party.id, name: party.name, role: "party" };
      }
      return null;
    },
    financialByName(name: string) {
      return accounts.find((account) => account.leaf && account.kind !== "category" && account.kind !== "equity" && account.kind !== "receivable" && account.kind !== "payable" && account.name === name) ?? null;
    },
    leafCategory(entityId: string, name: string) {
      const found = accounts.find((account) => account.kind === "category" && account.entityId === entityId && account.name.toLowerCase() === name.toLowerCase());
      if (!found || !found.leaf) {
        return null;
      }
      return found;
    },
    accountById(id: string) {
      return byId().get(id) ?? null;
    },
    ensureEquity(entityId: string) {
      const existing = accounts.find((account) => account.entityId === entityId && account.kind === "equity");
      if (existing) {
        return existing;
      }
      return addAccount({
        id: uniqueId(`acct_opening_equity_${slug(entityId)}`),
        name: "Opening equity",
        entityId,
        accountType: "equity",
        kind: "equity",
        parentId: null,
        leaf: true,
      });
    },
    ensureDueFrom(ownerEntityId: string, counterpartyId: string, counterpartyName: string) {
      const existing = accounts.find((account) => account.kind === "receivable" && account.entityId === ownerEntityId && account.id.endsWith(counterpartyId));
      if (existing) {
        return existing;
      }
      return addAccount({
        id: uniqueId(`acct_due_from_${slug(ownerEntityId)}_${slug(counterpartyId)}`),
        name: `Due from ${counterpartyName}`,
        entityId: ownerEntityId,
        accountType: "asset",
        kind: "receivable",
        parentId: null,
        leaf: true,
      });
    },
    ensureDueTo(ownerEntityId: string, counterpartyId: string, counterpartyName: string) {
      const existing = accounts.find((account) => account.kind === "payable" && account.entityId === ownerEntityId && account.id.endsWith(slug(counterpartyId)));
      if (existing) {
        return existing;
      }
      return addAccount({
        id: uniqueId(`acct_due_to_${slug(ownerEntityId)}_${slug(counterpartyId)}`),
        name: `Due to ${counterpartyName}`,
        entityId: ownerEntityId,
        accountType: "liability",
        kind: "payable",
        parentId: null,
        leaf: true,
      });
    },
    ensureInferredFinancial(name: string) {
      const existing = chart.financialByName(name);
      if (existing) {
        return existing;
      }
      const inferred = inferFinancial(name);
      return addAccount({
        id: uniqueId(`acct_${slug(name)}`),
        name,
        entityId: entities[0].id,
        accountType: inferred.accountType,
        kind: inferred.kind,
        parentId: null,
        leaf: true,
      });
    },
    ensureInferredCategory(entityId: string, name: string) {
      const existing = accounts.find((account) => account.kind === "category" && account.entityId === entityId && account.name.toLowerCase() === name.toLowerCase());
      if (existing) {
        return existing;
      }
      return addAccount({
        id: uniqueId(`cat_${slug(entityId)}_${slug(name)}`),
        name,
        entityId,
        accountType: inferCategoryType(name),
        kind: "category",
        parentId: null,
        leaf: true,
      });
    },
  };
  return chart;
}
