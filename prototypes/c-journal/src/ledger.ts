import { CorrectionError } from "./errors.ts";
import { addMinor } from "./money.ts";
import {
  activeEventFor,
  categoryAccountId,
  eventMagnitude,
  lineageFor,
  naturalBalance,
} from "./query.ts";
import type { CorrectionInput, IdSource, JournalEvent, Ledger, LedgerAccount, Posting, Side } from "./types.ts";

interface Spec {
  accountId: string;
  entityId: string;
  side: Side;
  amountMinor: number;
}

function scratchIds(): IdSource {
  let n = 0;
  return {
    event() {
      n += 1;
      return `preview_je_${n}`;
    },
    posting() {
      n += 1;
      return `preview_pst_${n}`;
    },
    stamp() {
      n += 1;
      return new Date(Date.parse("2025-11-02T16:00:00.000Z") + n * 1000).toISOString();
    },
  };
}

function materialize(ids: IdSource, draft: {
  effectiveOn: string;
  memo: string;
  observationIds: string[];
  reversesId: string | null;
  reason: string;
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
      throw new CorrectionError("That correction does not balance.");
    }
  }
  return {
    id: ids.event(),
    recordedAt: ids.stamp(),
    effectiveOn: draft.effectiveOn,
    memo: draft.memo,
    observationIds: draft.observationIds.slice(),
    reversesId: draft.reversesId,
    supersededBy: null,
    reason: draft.reason,
    postings,
  };
}

function assertObligations(events: JournalEvent[], accounts: LedgerAccount[]): void {
  for (const account of accounts) {
    if (account.kind !== "receivable" && account.kind !== "payable") {
      continue;
    }
    if (naturalBalance(events, account, null) < 0) {
      throw new CorrectionError(
        account.kind === "receivable"
          ? "That correction would take a receivable below zero."
          : "That correction would take a payable below zero.",
      );
    }
  }
}

function buildCorrection(ledger: Ledger, input: CorrectionInput, ids: IdSource): { reversal: JournalEvent; replacement: JournalEvent } {
  const target = ledger.events.find((event) => event.id === input.eventId);
  if (!target || target.observationIds.length === 0) {
    throw new CorrectionError("Only an entry that interprets an observation can be corrected.");
  }
  if (target.supersededBy !== null || target.reversesId !== null) {
    throw new CorrectionError("Correct the current entry. Reversals and superseded entries stay as history.");
  }
  const reason = input.reason.trim();
  if (reason === "") {
    throw new CorrectionError("A correction needs a reason.");
  }
  if (reason.length > 200) {
    throw new CorrectionError("The reason is too long.");
  }
  if (!Number.isSafeInteger(input.amountMinor) || input.amountMinor <= 0) {
    throw new CorrectionError("Enter an amount greater than zero.");
  }
  const magnitude = eventMagnitude(target);
  if (magnitude === null) {
    throw new CorrectionError("This prototype only corrects entries whose lines share one amount.");
  }
  const currentCategory = categoryAccountId(target, ledger.accounts);
  const nextCategory = input.categoryAccountId === null
    ? null
    : ledger.accounts.find((account) => account.id === input.categoryAccountId) ?? null;
  if (input.categoryAccountId !== null) {
    if (currentCategory === null || !nextCategory || !nextCategory.leaf) {
      throw new CorrectionError("Pick a leaf income or expense account.");
    }
    if (nextCategory.accountType !== "income" && nextCategory.accountType !== "expense") {
      throw new CorrectionError("Pick a leaf income or expense account.");
    }
    const current = ledger.accounts.find((account) => account.id === currentCategory);
    if (!current || nextCategory.entityId !== current.entityId || nextCategory.accountType !== current.accountType) {
      throw new CorrectionError("The category has to stay on the same entity and the same side of the ledger.");
    }
  }
  if (magnitude === input.amountMinor && (input.categoryAccountId === null || input.categoryAccountId === currentCategory)) {
    throw new CorrectionError("Change the amount or the category before posting a correction.");
  }

  const reversalSpecs: Spec[] = target.postings.map((posting) => ({
    accountId: posting.accountId,
    entityId: posting.entityId,
    side: posting.side === "debit" ? "credit" : "debit",
    amountMinor: posting.amountMinor,
  }));
  const replacementSpecs: Spec[] = target.postings.map((posting) => {
    if (currentCategory !== null && posting.accountId === currentCategory && nextCategory) {
      return { accountId: nextCategory.id, entityId: nextCategory.entityId, side: posting.side, amountMinor: input.amountMinor };
    }
    return { accountId: posting.accountId, entityId: posting.entityId, side: posting.side, amountMinor: input.amountMinor };
  });

  const reversal = materialize(ids, {
    effectiveOn: target.effectiveOn,
    memo: `Reversal of ${target.id}`,
    observationIds: target.observationIds,
    reversesId: target.id,
    reason,
    specs: reversalSpecs,
  });
  const replacement = materialize(ids, {
    effectiveOn: target.effectiveOn,
    memo: target.memo,
    observationIds: target.observationIds,
    reversesId: null,
    reason,
    specs: replacementSpecs,
  });
  const projected = ledger.events
    .map((event) => (event.id === target.id ? { ...event, supersededBy: reversal.id } : event))
    .concat([reversal, replacement]);
  assertObligations(projected, ledger.accounts);
  return { reversal, replacement };
}

export function previewCorrection(ledger: Ledger, input: CorrectionInput): { reversal: JournalEvent; replacement: JournalEvent } {
  return buildCorrection(ledger, input, scratchIds());
}

export function commitCorrection(ledger: Ledger, input: CorrectionInput): { reversalId: string; replacementId: string } {
  const built = buildCorrection(ledger, input, ledger.ids);
  const target = ledger.events.find((event) => event.id === input.eventId);
  if (!target || target.supersededBy !== null) {
    throw new CorrectionError("That entry was already superseded.");
  }
  target.supersededBy = built.reversal.id;
  ledger.events.push(built.reversal, built.replacement);
  return { reversalId: built.reversal.id, replacementId: built.replacement.id };
}

export type DemonstrationState = "absent" | "ready" | "done" | "diverged";

export function demonstrationState(ledger: Ledger): DemonstrationState {
  const demo = ledger.demonstration;
  if (!demo) {
    return "absent";
  }
  const active = activeEventFor(ledger.events, demo.observationId);
  if (!active) {
    return "diverged";
  }
  const amount = eventMagnitude(active);
  const category = categoryAccountId(active, ledger.accounts);
  const last = demo.steps[demo.steps.length - 1];
  if (amount === last.amountMinor && category === last.categoryId) {
    const lineage = lineageFor(ledger.events, demo.observationId);
    if (lineage.length === 1 + demo.steps.length * 2) {
      return "done";
    }
  }
  if (amount !== null && demo.steps.some((step) => step.fromAmountMinor === amount)) {
    return "ready";
  }
  return "diverged";
}

export function applyDemonstration(ledger: Ledger): number {
  const demo = ledger.demonstration;
  if (!demo) {
    return 0;
  }
  let applied = 0;
  for (const step of demo.steps) {
    const active = activeEventFor(ledger.events, demo.observationId);
    if (!active) {
      break;
    }
    if (eventMagnitude(active) !== step.fromAmountMinor) {
      continue;
    }
    commitCorrection(ledger, {
      eventId: active.id,
      amountMinor: step.amountMinor,
      categoryAccountId: step.categoryId,
      reason: step.reason,
    });
    applied += 1;
  }
  return applied;
}
