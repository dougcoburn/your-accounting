import { readObservations } from "./adapter.ts";
import type { BooksFile, WorkingChart } from "./chart.ts";
import { loadChart } from "./chart.ts";
import { FeedError, PostingError } from "./errors.ts";
import { addMinor } from "./money.ts";
import { naturalBalance } from "./query.ts";
import type {
  IdSource,
  JournalEvent,
  Ledger,
  LedgerAccount,
  Observation,
  Posting,
  Side,
  SourceKind,
  Unposted,
} from "./types.ts";

interface Spec {
  accountId: string;
  entityId: string;
  side: Side;
  amountMinor: number;
}

const TRANSFER_CATEGORIES = new Set(["transfer", "credit card payment", "card payment"]);

function makeIds(): IdSource {
  let events = 0;
  let postings = 0;
  let clock = 0;
  const start = Date.parse("2025-11-02T16:00:00.000Z");
  return {
    event() {
      events += 1;
      return `je_${String(events).padStart(4, "0")}`;
    },
    posting() {
      postings += 1;
      return `pst_${String(postings).padStart(4, "0")}`;
    },
    stamp() {
      const iso = new Date(start + clock * 60_000).toISOString();
      clock += 1;
      return iso;
    },
  };
}

function spec(account: LedgerAccount, side: Side, amountMinor: number): Spec {
  if (!Number.isSafeInteger(amountMinor) || amountMinor <= 0) {
    throw new PostingError("Posting amount must be a positive integer.");
  }
  if ((account.accountType === "income" || account.accountType === "expense") && !account.leaf) {
    throw new PostingError("Categories only post to leaf accounts.");
  }
  return { accountId: account.id, entityId: account.entityId, side, amountMinor };
}

function assertBalanced(postings: Posting[]): void {
  if (postings.length < 2) {
    throw new PostingError("Unbalanced journal event.");
  }
  const totals = new Map<string, { debit: number; credit: number }>();
  for (const posting of postings) {
    const bucket = totals.get(posting.entityId) ?? { debit: 0, credit: 0 };
    if (posting.side === "debit") {
      bucket.debit = addMinor(bucket.debit, posting.amountMinor);
    } else {
      bucket.credit = addMinor(bucket.credit, posting.amountMinor);
    }
    totals.set(posting.entityId, bucket);
  }
  for (const bucket of totals.values()) {
    if (bucket.debit !== bucket.credit || bucket.debit <= 0) {
      throw new PostingError("Unbalanced journal event.");
    }
  }
}

function makeEvent(ids: IdSource, draft: {
  effectiveOn: string;
  memo: string;
  observationIds: string[];
  reversesId: string | null;
  reason: string | null;
  specs: Spec[];
}): JournalEvent {
  const postings: Posting[] = draft.specs.map((item) => ({
    id: ids.posting(),
    accountId: item.accountId,
    entityId: item.entityId,
    side: item.side,
    amountMinor: item.amountMinor,
    currency: "USD",
  }));
  assertBalanced(postings);
  return {
    id: ids.event(),
    recordedAt: ids.stamp(),
    effectiveOn: draft.effectiveOn,
    memo: draft.memo,
    observationIds: draft.observationIds,
    reversesId: draft.reversesId,
    supersededBy: null,
    reason: draft.reason,
    postings,
  };
}

function isTransfer(category: string): boolean {
  return TRANSFER_CATEGORIES.has(category.trim().toLowerCase());
}

function entityTags(observation: Observation): string[] {
  return observation.tags.filter((tag) => tag.toLowerCase().startsWith("e-"));
}

function monthEnd(yearMonth: string): string {
  const [year, month] = yearMonth.split("-").map(Number);
  const last = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return `${yearMonth}-${String(last).padStart(2, "0")}`;
}

function chooseCutoff(observations: Observation[], fallback: string | null): string {
  if (fallback) {
    return fallback;
  }
  const counts = new Map<string, number>();
  for (const observation of observations) {
    const month = observation.occurredOn.slice(0, 7);
    counts.set(month, (counts.get(month) ?? 0) + 1);
  }
  let best = observations[0].occurredOn.slice(0, 7);
  for (const [month, count] of counts) {
    const bestCount = counts.get(best) ?? 0;
    if (count > bestCount || (count === bestCount && month > best)) {
      best = month;
    }
  }
  return monthEnd(best);
}

function financialAccount(chart: WorkingChart, name: string): LedgerAccount | null {
  if (chart.strict) {
    return chart.financialByName(name);
  }
  return chart.ensureInferredFinancial(name);
}

function categoryAccount(chart: WorkingChart, entityId: string, name: string): LedgerAccount | null {
  if (!chart.strict) {
    return chart.ensureInferredCategory(entityId, name);
  }
  return chart.leafCategory(entityId, name);
}

function entityName(chart: WorkingChart, entityId: string): string {
  return chart.entities.find((entity) => entity.id === entityId)?.name ?? "entity";
}

function findCounterpart(observation: Observation, all: Observation[], consumed: Set<string>): Observation | null {
  const candidates = all.filter((other) =>
    other.id !== observation.id &&
    !consumed.has(other.id) &&
    other.occurredOn === observation.occurredOn &&
    other.amountMinor === -observation.amountMinor &&
    other.amountMinor !== 0 &&
    other.accountName !== observation.accountName &&
    isTransfer(other.category)
  );
  candidates.sort((left, right) => (left.id < right.id ? -1 : left.id > right.id ? 1 : 0));
  return candidates[0] ?? null;
}

function pnlSpecs(account: LedgerAccount, category: LedgerAccount, amountMinor: number): Spec[] {
  const magnitude = Math.abs(amountMinor);
  if (amountMinor < 0) {
    return [spec(category, "debit", magnitude), spec(account, "credit", magnitude)];
  }
  return [spec(account, "debit", magnitude), spec(category, "credit", magnitude)];
}

export function buildLedger(csv: string, books: BooksFile | null, source: SourceKind): Ledger {
  if (source === "fixture" && (books === null || !csv.includes("# synthetic"))) {
    throw new FeedError(books === null ? "The fixture chart could not be read." : "The fixture is missing its synthetic marker.");
  }
  const chart = loadChart(books);
  const observations = readObservations(csv, books ? books.postedDates : null);
  const ids = makeIds();
  const events: JournalEvent[] = [];
  const unposted: Unposted[] = [];
  const consumed = new Set<string>();

  for (const opening of chart.openings) {
    const account = chart.accountById(opening.accountId);
    if (!account || opening.amountMinor <= 0) {
      throw new FeedError("The fixture chart has an opening that is not usable.");
    }
    const equity = chart.ensureEquity(account.entityId);
    const specs = account.accountType === "asset"
      ? [spec(account, "debit", opening.amountMinor), spec(equity, "credit", opening.amountMinor)]
      : [spec(equity, "debit", opening.amountMinor), spec(account, "credit", opening.amountMinor)];
    events.push(makeEvent(ids, {
      effectiveOn: opening.openingOn,
      memo: "Opening balance",
      observationIds: [],
      reversesId: null,
      reason: null,
      specs,
    }));
  }

  const sorted = observations.slice().sort((left, right) => {
    if (left.occurredOn !== right.occurredOn) {
      return left.occurredOn < right.occurredOn ? -1 : 1;
    }
    return left.id < right.id ? -1 : left.id > right.id ? 1 : 0;
  });

  for (const observation of sorted) {
    if (consumed.has(observation.id)) {
      continue;
    }
    if (observation.amountMinor === 0) {
      unposted.push({ observationId: observation.id, reason: "A zero amount is not posted." });
      consumed.add(observation.id);
      continue;
    }
    const account = financialAccount(chart, observation.accountName);
    if (!account) {
      unposted.push({ observationId: observation.id, reason: "The account name is not in the chart." });
      consumed.add(observation.id);
      continue;
    }
    const magnitude = Math.abs(observation.amountMinor);
    const memo = observation.merchant || "Journal entry";

    try {
      if (isTransfer(observation.category)) {
        const other = findCounterpart(observation, sorted, consumed);
        if (!other) {
          unposted.push({
            observationId: observation.id,
            reason: "A transfer with no counterpart stays out of the journal. Nothing is parked in a clearing account.",
          });
          consumed.add(observation.id);
          continue;
        }
        const otherAccount = financialAccount(chart, other.accountName);
        if (!otherAccount) {
          unposted.push({ observationId: observation.id, reason: "The account name is not in the chart." });
          unposted.push({ observationId: other.id, reason: "The account name is not in the chart." });
          consumed.add(observation.id);
          consumed.add(other.id);
          continue;
        }
        const inflowAccount = observation.amountMinor < 0 ? otherAccount : account;
        const outflowAccount = observation.amountMinor < 0 ? account : otherAccount;
        events.push(makeEvent(ids, {
          effectiveOn: observation.occurredOn,
          memo: `Transfer · ${outflowAccount.name} / ${inflowAccount.name}`,
          observationIds: [observation.id, other.id].sort(),
          reversesId: null,
          reason: null,
          specs: [spec(inflowAccount, "debit", magnitude), spec(outflowAccount, "credit", magnitude)],
        }));
        consumed.add(observation.id);
        consumed.add(other.id);
        continue;
      }

      if (observation.category.trim().toLowerCase() === "reimbursement") {
        const tags = entityTags(observation);
        if (observation.amountMinor < 0 || tags.length !== 1) {
          unposted.push({
            observationId: observation.id,
            reason: tags.length === 1
              ? "This reimbursement shape is not posted by the prototype."
              : "A reimbursement needs an e-* tag.",
          });
          consumed.add(observation.id);
          continue;
        }
        const target = chart.describeTag(tags[0]);
        if (!target) {
          unposted.push({ observationId: observation.id, reason: "The e-* tag is not in the chart." });
          consumed.add(observation.id);
          continue;
        }
        const existing = chart.accounts.find((item) => item.kind === "receivable" && item.entityId === account.entityId && item.name === `Due from ${target.name}`);
        const open = existing ? naturalBalance(events, existing, null) : 0;
        if (addMinor(open, -magnitude) < 0) {
          unposted.push({ observationId: observation.id, reason: "A reimbursement larger than the open claim is not posted." });
          consumed.add(observation.id);
          continue;
        }
        const due = chart.ensureDueFrom(account.entityId, target.id, target.name);
        events.push(makeEvent(ids, {
          effectiveOn: observation.occurredOn,
          memo: `${memo} · settles claim on ${target.name}`,
          observationIds: [observation.id],
          reversesId: null,
          reason: null,
          specs: [spec(account, "debit", magnitude), spec(due, "credit", magnitude)],
        }));
        consumed.add(observation.id);
        continue;
      }

      const tags = entityTags(observation);
      if (tags.length > 1) {
        unposted.push({ observationId: observation.id, reason: "More than one e-* tag is on this row." });
        consumed.add(observation.id);
        continue;
      }
      if (tags.length === 1) {
        const target = chart.describeTag(tags[0]);
        if (!target) {
          unposted.push({ observationId: observation.id, reason: "The e-* tag is not in the chart." });
          consumed.add(observation.id);
          continue;
        }
        if (!(target.role === "entity" && target.id === account.entityId)) {
          const due = chart.ensureDueFrom(account.entityId, target.id, target.name);
          const payer = observation.amountMinor < 0
            ? [spec(due, "debit", magnitude), spec(account, "credit", magnitude)]
            : [spec(account, "debit", magnitude), spec(due, "credit", magnitude)];
          let specs = payer;
          let entryMemo = `${memo} · claim on ${target.name}`;
          if (target.role === "entity") {
            const category = categoryAccount(chart, target.id, observation.category);
            if (!category) {
              unposted.push({ observationId: observation.id, reason: "No leaf category is available for this entity." });
              consumed.add(observation.id);
              continue;
            }
            const dueTo = chart.ensureDueTo(target.id, account.entityId, entityName(chart, account.entityId));
            const mirror = observation.amountMinor < 0
              ? [spec(category, "debit", magnitude), spec(dueTo, "credit", magnitude)]
              : [spec(dueTo, "debit", magnitude), spec(category, "credit", magnitude)];
            specs = [...payer, ...mirror];
            entryMemo = `${memo} · for ${target.name}`;
          }
          events.push(makeEvent(ids, {
            effectiveOn: observation.occurredOn,
            memo: entryMemo,
            observationIds: [observation.id],
            reversesId: null,
            reason: null,
            specs,
          }));
          consumed.add(observation.id);
          continue;
        }
      }

      const category = categoryAccount(chart, account.entityId, observation.category);
      if (!category) {
        unposted.push({ observationId: observation.id, reason: "No leaf category is available for this entity." });
        consumed.add(observation.id);
        continue;
      }
      events.push(makeEvent(ids, {
        effectiveOn: observation.occurredOn,
        memo,
        observationIds: [observation.id],
        reversesId: null,
        reason: null,
        specs: pnlSpecs(account, category, observation.amountMinor),
      }));
      consumed.add(observation.id);
    } catch (error) {
      if (error instanceof PostingError) {
        unposted.push({ observationId: observation.id, reason: "The row did not make a balanced entry, so it was not posted." });
        consumed.add(observation.id);
        continue;
      }
      throw error;
    }
  }

  return {
    source,
    currency: "USD",
    entities: chart.entities,
    parties: chart.parties,
    accounts: chart.accounts,
    observations,
    events,
    unposted,
    statements: chart.statements,
    defaultCutoff: chooseCutoff(observations, chart.defaultCutoff),
    demonstration: chart.demonstration,
    ids,
  };
}
