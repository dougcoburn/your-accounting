import { addMinor } from "./money.ts";
import type { JournalEvent, LedgerAccount, Side } from "./types.ts";

export function isActive(event: JournalEvent): boolean {
  return event.supersededBy === null && event.reversesId === null;
}

export function naturalDelta(account: LedgerAccount, side: Side, amountMinor: number): number {
  const debitNormal = account.accountType === "asset" || account.accountType === "expense";
  const increasing = debitNormal ? side === "debit" : side === "credit";
  return increasing ? amountMinor : -amountMinor;
}

export function naturalBalance(events: JournalEvent[], account: LedgerAccount, cutoff: string | null): number {
  let total = 0;
  for (const event of events) {
    if (cutoff !== null && event.effectiveOn > cutoff) {
      continue;
    }
    for (const posting of event.postings) {
      if (posting.accountId !== account.id) {
        continue;
      }
      total = addMinor(total, naturalDelta(account, posting.side, posting.amountMinor));
    }
  }
  return total;
}

export function trialTotals(events: JournalEvent[]): { debit: number; credit: number } {
  let debit = 0;
  let credit = 0;
  for (const event of events) {
    for (const posting of event.postings) {
      if (posting.side === "debit") {
        debit = addMinor(debit, posting.amountMinor);
      } else {
        credit = addMinor(credit, posting.amountMinor);
      }
    }
  }
  return { debit, credit };
}

export interface Equation {
  entityId: string;
  assets: number;
  liabilities: number;
  equity: number;
  income: number;
  expenses: number;
  discrepancy: number;
}

export function equations(events: JournalEvent[], accounts: LedgerAccount[], entityIds: string[]): Equation[] {
  return entityIds.map((entityId) => {
    const totals = { asset: 0, liability: 0, equity: 0, income: 0, expense: 0 };
    for (const account of accounts) {
      if (account.entityId !== entityId) {
        continue;
      }
      totals[account.accountType] = addMinor(totals[account.accountType], naturalBalance(events, account, null));
    }
    const right = addMinor(addMinor(totals.liability, totals.equity), addMinor(totals.income, -totals.expense));
    return {
      entityId,
      assets: totals.asset,
      liabilities: totals.liability,
      equity: totals.equity,
      income: totals.income,
      expenses: totals.expense,
      discrepancy: addMinor(totals.asset, -right),
    };
  });
}

export function sideTotals(events: JournalEvent[], accountId: string): { debit: number; credit: number } {
  let debit = 0;
  let credit = 0;
  for (const event of events) {
    for (const posting of event.postings) {
      if (posting.accountId !== accountId) {
        continue;
      }
      if (posting.side === "debit") {
        debit = addMinor(debit, posting.amountMinor);
      } else {
        credit = addMinor(credit, posting.amountMinor);
      }
    }
  }
  return { debit, credit };
}

export function isFinancial(account: LedgerAccount): boolean {
  return account.kind === "cash" || account.kind === "card" || account.kind === "investment";
}

export function accountPath(accounts: LedgerAccount[], account: LedgerAccount): string {
  const names = [account.name];
  const byId = new Map(accounts.map((item) => [item.id, item]));
  const seen = new Set<string>();
  let parentId = account.parentId;
  while (parentId) {
    if (seen.has(parentId)) {
      break;
    }
    seen.add(parentId);
    const parent = byId.get(parentId);
    if (!parent) {
      break;
    }
    names.push(parent.name);
    parentId = parent.parentId;
  }
  return names.reverse().join(" / ");
}

export function lineageFor(events: JournalEvent[], observationId: string): JournalEvent[] {
  return events
    .filter((event) => event.observationIds.includes(observationId))
    .slice()
    .sort((left, right) => {
      if (left.recordedAt !== right.recordedAt) {
        return left.recordedAt < right.recordedAt ? -1 : 1;
      }
      return left.id < right.id ? -1 : left.id > right.id ? 1 : 0;
    });
}

export function activeEventFor(events: JournalEvent[], observationId: string): JournalEvent | null {
  const active = events.filter((event) => event.observationIds.includes(observationId) && isActive(event));
  active.sort((left, right) => (left.recordedAt < right.recordedAt ? 1 : left.recordedAt > right.recordedAt ? -1 : 0));
  return active[0] ?? null;
}

export function eventMagnitude(event: JournalEvent): number | null {
  const first = event.postings[0];
  if (!first) {
    return null;
  }
  if (event.postings.some((posting) => posting.amountMinor !== first.amountMinor)) {
    return null;
  }
  return first.amountMinor;
}

export function categoryAccountId(event: JournalEvent, accounts: LedgerAccount[]): string | null {
  const matches = event.postings.filter((posting) => {
    const account = accounts.find((item) => item.id === posting.accountId);
    return account !== undefined && (account.accountType === "income" || account.accountType === "expense");
  });
  if (matches.length === 0) {
    return null;
  }
  const id = matches[0].accountId;
  if (matches.some((posting) => posting.accountId !== id)) {
    return null;
  }
  return id;
}

export function supersedeProblems(events: JournalEvent[]): string[] {
  const byId = new Map(events.map((event) => [event.id, event]));
  const problems: string[] = [];
  for (const event of events) {
    if (event.supersededBy !== null) {
      const reversal = byId.get(event.supersededBy);
      if (!reversal || reversal.reversesId !== event.id) {
        problems.push("superseded_by does not match a reversal");
      }
    }
    if (event.reversesId !== null) {
      const original = byId.get(event.reversesId);
      if (!original || original.supersededBy !== event.id) {
        problems.push("reversal link does not match superseded_by");
      }
    }
  }
  return problems;
}
