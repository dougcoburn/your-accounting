import { formatUsd } from "./money.ts";
import { suggestAll } from "./suggestions.ts";
import type {
  Books,
  ExpectedMovement,
  JournalEntry,
  JournalPosting,
  Line,
  Observation,
  Progress,
  RecipeId,
  SettlementGroupSuggestion,
  Treatment,
} from "./types.ts";

function observedKey(id: string): string {
  return `obs:${id}`;
}

function expectedKey(seq: number): string {
  return `exp:${seq}`;
}

export type BooksOptions = {
  /** When true, Monarch Reviewed=true starts reconciled. Default is to ignore it. */
  honorMonarchReviewed?: boolean;
};

export function booksFromObservations(
  observations: Observation[],
  options: BooksOptions = {},
): Books {
  const honor = options.honorMonarchReviewed === true;
  const lines: Line[] = observations.map((obs) => {
    const startReconciled = honor && obs.reviewed;
    return {
      key: observedKey(obs.id),
      origin: "observed" as const,
      state: startReconciled ? "reconciled" : "unreviewed",
      observation: obs,
      appliedRecipe: startReconciled ? "imported-reviewed" : undefined,
    };
  });
  const entries: JournalEntry[] = [];
  let entrySeq = 1;
  let groupSeq = 1;
  // Already-reviewed observations land as simple opening entries so the
  // toy ledger is not empty. Grouping is not reconstructed.
  for (const line of lines) {
    if (line.state !== "reconciled" || !line.observation) continue;
    const obs = line.observation;
    const treatment = suggestAll([obs]).get(obs.id);
    const entry = entryFromSimple(obs, treatment, `grp-open-${groupSeq}`, `je-${entrySeq}`);
    if (entry) {
      entries.push(entry);
      entrySeq += 1;
      groupSeq += 1;
    }
  }
  return { lines, entries, entrySeq, expectedSeq: 1, groupSeq };
}

function entryFromSimple(
  obs: Observation,
  treatment: Treatment | undefined,
  groupId: string,
  entryId: string,
): JournalEntry | undefined {
  const recipeId: RecipeId = treatment?.recipeId ?? (obs.amountMinor < 0 ? "purchase" : "refund");
  const recipeLabel = treatment?.recipeLabel ?? "Purchase";
  const category = treatment?.category || obs.category || "Uncategorized";
  const postings = postingsForSimple(recipeId, obs, category);
  if (!postings) return undefined;
  return {
    id: entryId,
    date: obs.date,
    recipeId,
    recipeLabel,
    memo: `${obs.merchant} · ${obs.account}`,
    settlementGroupId: groupId,
    postings,
    sourceKeys: [observedKey(obs.id)],
  };
}

function postingsForSimple(recipeId: RecipeId, obs: Observation, category: string): JournalPosting[] | undefined {
  const abs = Math.abs(obs.amountMinor);
  if (abs === 0) return undefined;
  const financial = obs.account;
  if (recipeId === "purchase" && obs.amountMinor < 0) {
    return [
      { account: `Expense · ${category}`, debitMinor: abs, creditMinor: 0 },
      { account: financial, debitMinor: 0, creditMinor: abs },
    ];
  }
  if (recipeId === "refund" || recipeId === "perk-credit" || (recipeId === "purchase" && obs.amountMinor > 0)) {
    const cat = recipeId === "perk-credit" ? "Expense · Card perks" : `Expense · ${category}`;
    return [
      { account: financial, debitMinor: abs, creditMinor: 0 },
      { account: cat, debitMinor: 0, creditMinor: abs },
    ];
  }
  return undefined;
}

function postingsForGroup(treatment: Treatment, primary: Observation): JournalPosting[] {
  const g = treatment.settlementGroup;
  if (!g) return postingsForSimple(treatment.recipeId, primary, treatment.category) ?? [];

  if (g.recipeId === "card-payment") {
    const bank = g.participants.find((p) => p.role === "bank-outflow");
    const card = g.participants.find((p) => p.role === "card-payment");
    const amount = Math.abs(bank?.amountMinor ?? card?.amountMinor ?? primary.amountMinor);
    return [
      { account: card?.account || "Card", debitMinor: amount, creditMinor: 0 },
      { account: bank?.account || "Checking", debitMinor: 0, creditMinor: amount },
    ];
  }

  if (g.recipeId === "transfer") {
    const outLeg = g.participants.find((p) => p.role === "transfer-out");
    const inLeg = g.participants.find((p) => p.role === "transfer-in");
    const amount = Math.abs(outLeg?.amountMinor ?? inLeg?.amountMinor ?? primary.amountMinor);
    return [
      { account: inLeg?.account || "Destination", debitMinor: amount, creditMinor: 0 },
      { account: outLeg?.account || "Source", debitMinor: 0, creditMinor: amount },
    ];
  }

  if (g.recipeId === "paycheck") {
    const amt = (role: string) => Math.abs(g.participants.find((p) => p.role === role)?.amountMinor ?? 0);
    const netAccount = g.participants.find((p) => p.role === "net")?.account || primary.account;
    const hsaAccount = g.participants.find((p) => p.role === "hsa")?.account || "HSA";
    // Gross is credited (income). Uses of gross are debits. Expected HSA
    // is posted in its own draft group when filled, or left unfilled.
    return [
      { account: netAccount, debitMinor: amt("net"), creditMinor: 0 },
      { account: "Expense · Payroll taxes", debitMinor: amt("taxes"), creditMinor: 0 },
      { account: "Retirement · Pre-tax", debitMinor: amt("pre-tax"), creditMinor: 0 },
      { account: "Retirement · Roth", debitMinor: amt("roth"), creditMinor: 0 },
      { account: hsaAccount, debitMinor: amt("hsa"), creditMinor: 0 },
      { account: "Income · Wages", debitMinor: 0, creditMinor: amt("gross") },
    ];
  }

  return postingsForSimple(treatment.recipeId, primary, treatment.category) ?? [];
}

export function entryIsBalanced(entry: JournalEntry): boolean {
  const debits = entry.postings.reduce((s, p) => s + p.debitMinor, 0);
  const credits = entry.postings.reduce((s, p) => s + p.creditMinor, 0);
  return debits === credits;
}

function setState(lines: Line[], key: string, state: Line["state"], recipe?: string): void {
  const line = lines.find((l) => l.key === key);
  if (!line) return;
  line.state = state;
  if (recipe) line.appliedRecipe = recipe;
}

function occupied(line: Line | undefined): boolean {
  if (!line) return false;
  return line.state === "reconciled" || line.state === "voided";
}

export type ApplyResult = {
  books: Books;
  ok: boolean;
  reason?: string;
};

export function applyTreatment(books: Books, observationId: string, treatment: Treatment): ApplyResult {
  const lines = books.lines.map((l) => ({ ...l }));
  const primaryKey = observedKey(observationId);
  const primary = lines.find((l) => l.key === primaryKey);
  if (!primary?.observation) return { books, ok: false, reason: "missing observation" };
  if (occupied(primary) || primary.state === "open") {
    return { books, ok: false, reason: "line is not applyable from inbox" };
  }

  const participantIds = (treatment.settlementGroup?.participants ?? [])
    .map((p) => p.observationId)
    .filter((id): id is string => Boolean(id));
  for (const id of participantIds) {
    if (id === observationId) continue;
    const other = lines.find((l) => l.key === observedKey(id));
    if (occupied(other)) {
      return { books, ok: false, reason: "a participant is already reconciled or voided" };
    }
  }

  const groupId = `grp-${books.groupSeq}`;
  const entryId = `je-${books.entrySeq}`;
  const postings = postingsForGroup(treatment, primary.observation);
  const entry: JournalEntry = {
    id: entryId,
    date: primary.observation.date,
    recipeId: treatment.recipeId,
    recipeLabel: treatment.recipeLabel,
    memo: `${primary.observation.merchant} · ${treatment.summary}`,
    settlementGroupId: groupId,
    postings,
    sourceKeys: [primaryKey, ...participantIds.filter((id) => id !== observationId).map(observedKey)],
  };
  if (!entryIsBalanced(entry)) {
    return { books, ok: false, reason: "unbalanced journal entry" };
  }

  // Paycheck: observed net reconciles. Expected HSA is its own draft group.
  if (treatment.recipeId === "paycheck") {
    setState(lines, primaryKey, "reconciled", treatment.recipeLabel);
    const slot = treatment.settlementGroup?.participants.find((p) => p.role === "expected-hsa");
    let expectedSeq = books.expectedSeq;
    const extraEntries: JournalEntry[] = [];
    if (slot?.filled && slot.observationId) {
      setState(lines, observedKey(slot.observationId), "reconciled", "Expected HSA");
      extraEntries.push({
        id: `je-${books.entrySeq + 1}`,
        date: primary.observation.date,
        recipeId: "paycheck",
        recipeLabel: "Expected HSA",
        memo: `HSA inflow filling expected slot for ${primary.observation.merchant}`,
        settlementGroupId: `grp-${books.groupSeq + 1}`,
        postings: [
          { account: slot.account, debitMinor: Math.abs(slot.amountMinor), creditMinor: 0 },
          { account: "HSA withholding (clearing)", debitMinor: 0, creditMinor: Math.abs(slot.amountMinor) },
        ],
        sourceKeys: [observedKey(slot.observationId)],
      });
      // The paycheck already debited the HSA account for the employee
      // deduction. Filling the slot with a second debit would double-count.
      // Treat the fill as: the expected receipt lands, and the paycheck's
      // HSA debit is re-read as a transfer through withholding. For the toy
      // ledger we simply record the observed HSA inflow against a clearing
      // account that the paycheck HSA debit already hit? That would not
      // balance across groups.
      //
      // Cleaner toy-ledger choice: the paycheck posts HSA employee
      // deduction to "HSA withholding (clearing)", and the Expected HSA
      // group (when filled) is Debit HSA / Credit withholding. When the
      // slot is unfilled, withholding stays open as the expected movement.
    } else if (slot && !slot.filled) {
      const expected: ExpectedMovement = {
        key: expectedKey(expectedSeq),
        label: "Expected HSA",
        account: slot.account || "HSA",
        amountMinor: slot.amountMinor,
        date: primary.observation.date,
        relatedObservationId: observationId,
        recipeId: "paycheck",
      };
      expectedSeq += 1;
      lines.push({
        key: expected.key,
        origin: "expected",
        state: "open",
        expected,
        appliedRecipe: "Expected HSA (unfilled)",
      });
    }

    // Rewrite paycheck HSA posting to a withholding/clearing account so a
    // later (or concurrent) HSA fill does not double-debit the HSA.
    for (const p of entry.postings) {
      if (p.account && /hsa/i.test(p.account) && p.debitMinor > 0) {
        p.account = "HSA withholding (clearing)";
      }
    }

    const newEntries = [...books.entries, entry, ...extraEntries];
    return {
      books: {
        lines,
        entries: newEntries,
        entrySeq: books.entrySeq + 1 + extraEntries.length,
        expectedSeq,
        groupSeq: books.groupSeq + 1 + extraEntries.length,
      },
      ok: true,
    };
  }

  const groupComplete = treatment.settlementGroup ? treatment.settlementGroup.complete : true;
  const nextState = groupComplete ? "reconciled" : "interpreted";
  setState(lines, primaryKey, nextState, treatment.recipeLabel);
  for (const id of participantIds) {
    if (id === observationId) continue;
    setState(lines, observedKey(id), nextState, treatment.recipeLabel);
  }

  return {
    books: {
      lines,
      entries: [...books.entries, entry],
      entrySeq: books.entrySeq + 1,
      expectedSeq: books.expectedSeq,
      groupSeq: books.groupSeq + 1,
    },
    ok: true,
  };
}

export function skipToWorklist(books: Books, observationId: string): ApplyResult {
  const lines = books.lines.map((l) => ({ ...l }));
  const line = lines.find((l) => l.key === observedKey(observationId));
  if (!line) return { books, ok: false, reason: "missing line" };
  if (occupied(line)) return { books, ok: false, reason: "already closed" };
  line.state = "open";
  return { books: { ...books, lines }, ok: true };
}

export function voidLine(books: Books, observationId: string): ApplyResult {
  const lines = books.lines.map((l) => ({ ...l }));
  const line = lines.find((l) => l.key === observedKey(observationId));
  if (!line) return { books, ok: false, reason: "missing line" };
  if (line.state === "voided") return { books, ok: false, reason: "already voided" };
  line.state = "voided";
  line.appliedRecipe = "Void";
  return { books: { ...books, lines }, ok: true };
}

export function progress(books: Books): Progress {
  // Count observed lines only — expected slots are derived worklist items,
  // not imported observations the counter is driving toward.
  const observed = books.lines.filter((l) => l.origin === "observed");
  const count = (state: Line["state"]) => observed.filter((l) => l.state === state).length;
  const reconciled = count("reconciled");
  const voided = count("voided");
  const open = count("open");
  return {
    total: observed.length,
    unreviewed: count("unreviewed"),
    interpreted: count("interpreted"),
    reconciled,
    voided,
    open,
    reached: reconciled + voided + open,
  };
}

export function inboxLines(books: Books): Line[] {
  return books.lines
    .filter((l) => l.origin === "observed" && l.state === "unreviewed")
    .sort((a, b) => {
      const da = a.observation?.date ?? "";
      const db = b.observation?.date ?? "";
      if (da !== db) return da < db ? -1 : 1;
      return (a.observation?.id ?? "").localeCompare(b.observation?.id ?? "");
    });
}

export function worklistLines(books: Books): Line[] {
  return books.lines.filter((l) => {
    if (l.origin === "expected") return l.state !== "reconciled" && l.state !== "voided";
    return l.state === "open" || l.state === "interpreted";
  });
}

export function formatPosting(p: JournalPosting): string {
  if (p.debitMinor) return `${p.account}  dr ${formatUsd(p.debitMinor)}`;
  return `${p.account}  cr ${formatUsd(p.creditMinor)}`;
}
