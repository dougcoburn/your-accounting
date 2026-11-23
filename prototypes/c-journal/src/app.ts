import { parseBooks } from "./chart.ts";
import { el } from "./dom.ts";
import { CorrectionError, FeedError } from "./errors.ts";
import { applyDemonstration, commitCorrection, demonstrationState, previewCorrection } from "./ledger.ts";
import { formatMoney, formatSigned, formatStamp, minorToInput, parseSignedMinor } from "./money.ts";
import { buildLedger } from "./post.ts";
import {
  accountPath,
  activeEventFor,
  categoryAccountId,
  equations,
  eventMagnitude,
  isActive,
  isFinancial,
  lineageFor,
  naturalBalance,
  sideTotals,
  supersedeProblems,
  trialTotals,
} from "./query.ts";
import { reconcileAccount } from "./reconcile.ts";
import type { CorrectionInput, JournalEvent, Ledger, LedgerAccount, Observation, Posting, Side } from "./types.ts";

type ViewName = "journal" | "accounts" | "reconcile";

interface UiState {
  view: ViewName;
  audit: boolean;
  entityId: string;
  accountId: string;
  cutoff: string;
  endingRaw: string;
  status: string;
  openCorrectionId: string | null;
}

let ledger: Ledger;
const state: UiState = {
  view: "journal",
  audit: false,
  entityId: "all",
  accountId: "",
  cutoff: "",
  endingRaw: "",
  status: "",
  openCorrectionId: null,
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function ledgerFromPayload(payload: unknown): Ledger {
  if (!isRecord(payload) || (payload.source !== "fixture" && payload.source !== "env") || typeof payload.csv !== "string") {
    throw new FeedError("The feed could not be loaded.");
  }
  const books = payload.source === "fixture" ? parseBooks(payload.books) : null;
  return buildLedger(payload.csv, books, payload.source);
}

function showFatal(message: string): void {
  const root = document.getElementById("app");
  if (!root) {
    return;
  }
  root.replaceChildren(el("main", { class: "fatal" }, el("h1", null, "Journal"), el("p", null, message)));
}

function entityName(entityId: string): string {
  return ledger.entities.find((entity) => entity.id === entityId)?.name ?? entityId;
}

function accountById(id: string): LedgerAccount | null {
  return ledger.accounts.find((account) => account.id === id) ?? null;
}

function balanceLabel(account: LedgerAccount, natural: number): string {
  if (natural === 0) {
    return "0.00";
  }
  const debitNormal = account.accountType === "asset" || account.accountType === "expense";
  if (natural > 0) {
    return `${formatMoney(natural)} ${debitNormal ? "Dr" : "Cr"}`;
  }
  return `${formatMoney(-natural)} ${debitNormal ? "Cr" : "Dr"}`;
}

function visibleEvents(): JournalEvent[] {
  return ledger.events.filter((event) => {
    if (state.entityId !== "all" && !event.postings.some((posting) => posting.entityId === state.entityId)) {
      return false;
    }
    return true;
  });
}

function observationById(id: string): Observation | null {
  return ledger.observations.find((observation) => observation.id === id) ?? null;
}

function groupsOf(events: JournalEvent[]): JournalEvent[][] {
  const map = new Map<string, JournalEvent[]>();
  for (const event of events) {
    const key = event.observationIds.length > 0 ? event.observationIds.slice().sort().join("|") : `opening:${event.id}`;
    const list = map.get(key) ?? [];
    list.push(event);
    map.set(key, list);
  }
  const grouped = [...map.values()];
  grouped.sort((left, right) => {
    const leftEvent = left.find(isActive) ?? left[0];
    const rightEvent = right.find(isActive) ?? right[0];
    if (leftEvent.effectiveOn !== rightEvent.effectiveOn) {
      return leftEvent.effectiveOn < rightEvent.effectiveOn ? -1 : 1;
    }
    return leftEvent.id < rightEvent.id ? -1 : leftEvent.id > rightEvent.id ? 1 : 0;
  });
  return grouped;
}

function renderLine(posting: Posting): HTMLElement {
  const account = accountById(posting.accountId);
  const name = account ? accountPath(ledger.accounts, account) : posting.accountId;
  const entity = el("span", { class: "muted" }, ` · ${entityName(posting.entityId)}`);
  const label = el("span", { class: posting.side === "credit" ? "credit-name" : "" }, name);
  label.append(entity);
  const debit = el("span", { class: "num debit-amt" }, posting.side === "debit" ? formatMoney(posting.amountMinor) : "");
  const credit = el("span", { class: "num credit-amt" }, posting.side === "credit" ? formatMoney(posting.amountMinor) : "");
  return el("div", { class: "j-line" }, label, debit, credit);
}

function sourceFacts(event: JournalEvent): HTMLElement | null {
  const observations = event.observationIds.map(observationById).filter((observation): observation is Observation => observation !== null);
  if (observations.length === 0) {
    return null;
  }
  const rows: HTMLElement[] = [];
  for (const observation of observations) {
    const fields: Array<[string, string]> = [
      ["Id", observation.id],
      ["Occurred", observation.occurredOn],
      ["Posted", observation.postedOn ?? "unknown"],
      ["Merchant", observation.merchant],
      ["Amount", formatSigned(observation.amountMinor)],
      ["Category hint", observation.category],
      ["Account", observation.accountName],
      ["Tags", observation.tags.join(", ") || "none"],
      ["Notes", observation.notes || "none"],
      ["Statement text", observation.originalStatement || "none"],
    ];
    for (const [term, value] of fields) {
      rows.push(el("dt", null, term), el("dd", null, value));
    }
  }
  return el("details", null, el("summary", null, "Source fact"), el("dl", null, ...rows));
}

function categoryChoices(event: JournalEvent): LedgerAccount[] {
  const currentId = categoryAccountId(event, ledger.accounts);
  if (!currentId) {
    return [];
  }
  const current = accountById(currentId);
  if (!current) {
    return [];
  }
  return ledger.accounts
    .filter((account) => account.leaf && account.entityId === current.entityId && account.accountType === current.accountType && (account.accountType === "income" || account.accountType === "expense"))
    .slice()
    .sort((left, right) => accountPath(ledger.accounts, left).localeCompare(accountPath(ledger.accounts, right)));
}

function readCorrection(eventId: string): CorrectionInput {
  const amountInput = document.getElementById("correct-amount");
  const reasonInput = document.getElementById("correct-reason");
  const categoryInput = document.getElementById("correct-category");
  const amountRaw = amountInput instanceof HTMLInputElement ? amountInput.value : "";
  const reason = reasonInput instanceof HTMLInputElement ? reasonInput.value : "";
  let amountMinor: number;
  try {
    amountMinor = parseSignedMinor(amountRaw);
  } catch {
    throw new CorrectionError("Enter the amount in dollars and cents.");
  }
  if (amountMinor < 0) {
    amountMinor = -amountMinor;
  }
  const categoryAccountIdValue = categoryInput instanceof HTMLSelectElement && categoryInput.value !== "" ? categoryInput.value : null;
  return { eventId, amountMinor, categoryAccountId: categoryAccountIdValue, reason };
}

function paintPreview(eventId: string): void {
  const box = document.getElementById("correct-preview");
  const error = document.getElementById("correct-error");
  if (!box || !error) {
    return;
  }
  try {
    const preview = previewCorrection(ledger, readCorrection(eventId));
    box.replaceChildren(
      renderPreview("Reversal", preview.reversal),
      renderPreview("Replacement", preview.replacement),
    );
    error.textContent = "";
  } catch (caught) {
    box.replaceChildren();
    error.textContent = caught instanceof CorrectionError || caught instanceof FeedError
      ? caught.message
      : "That correction does not balance.";
  }
}

function renderPreview(title: string, event: JournalEvent): HTMLElement {
  const lines = event.postings.map((posting) => {
    const account = accountById(posting.accountId);
    const name = account ? accountPath(ledger.accounts, account) : posting.accountId;
    return el("p", null, `${posting.side === "debit" ? "Dr" : "Cr"} ${formatMoney(posting.amountMinor)}  ${name}`);
  });
  return el("div", null, el("h4", null, title), ...lines);
}

function renderCorrectionForm(event: JournalEvent): HTMLElement {
  const choices = categoryChoices(event);
  const magnitude = eventMagnitude(event) ?? 0;
  const currentCategory = categoryAccountId(event, ledger.accounts) ?? "";
  const amount = el("input", { id: "correct-amount", value: minorToInput(magnitude), inputmode: "decimal", autocomplete: "off" });
  const reason = el("input", { id: "correct-reason", maxlength: "200", autocomplete: "off" });
  const category = choices.length === 0
    ? null
    : el("select", { id: "correct-category" }, ...choices.map((account) => {
      const option = el("option", { value: account.id }, accountPath(ledger.accounts, account));
      if (account.id === currentCategory) {
        option.setAttribute("selected", "selected");
      }
      return option;
    }));
  const preview = el("div", { id: "correct-preview", class: "preview" });
  const error = el("p", { id: "correct-error", class: "out" }, "");
  const commit = el("button", { type: "button" }, "Append reversal and replacement");
  const form = el(
    "form",
    { class: "form" },
    el("h3", null, "Correct"),
    el("p", { class: "muted" }, "A correction appends a reversal and a replacement. It does not change the imported amount. The typed statement ending is left as you entered it, so a new amount inside a cutoff moves the difference."),
    el("label", null, "Journal amount", amount),
    category ? el("label", null, "Category", category) : null,
    el("label", null, "Reason", reason),
    preview,
    error,
    el("div", { class: "form-actions" }, commit),
  );
  form.addEventListener("submit", (event) => event.preventDefault());
  const onEdit = () => paintPreview(event.id);
  amount.addEventListener("input", onEdit);
  reason.addEventListener("input", onEdit);
  category?.addEventListener("change", onEdit);
  commit.addEventListener("click", () => {
    try {
      const result = commitCorrection(ledger, readCorrection(event.id));
      state.openCorrectionId = null;
      state.status = `Appended reversal ${result.reversalId} and replacement ${result.replacementId}. The prior entry is hidden until audit is on. The imported row is unchanged.`;
      render();
    } catch (caught) {
      error.textContent = caught instanceof CorrectionError || caught instanceof FeedError
        ? caught.message
        : "That correction does not balance.";
    }
  });
  return form;
}

function renderEventBlock(event: JournalEvent, history: boolean): HTMLElement {
  let mark = "";
  if (event.reversesId) {
    mark = `Reversal · reverses ${event.reversesId}`;
  } else if (event.supersededBy) {
    mark = `Superseded by ${event.supersededBy}`;
  } else if (state.audit) {
    mark = "Current";
  }
  const correct = !history && isActive(event) && event.observationIds.length > 0
    ? el("button", { type: "button" }, state.openCorrectionId === event.id ? "Close" : "Correct")
    : null;
  correct?.addEventListener("click", () => {
    state.openCorrectionId = state.openCorrectionId === event.id ? null : event.id;
    render();
  });
  const head = el(
    "div",
    { class: "entry-head" },
    el("div", null, el("strong", null, event.effectiveOn), el("span", { class: "muted" }, `  ${event.memo}`)),
    el("div", null, correct, mark ? el("span", { class: "kicker" }, mark) : null),
  );
  const lines = event.postings.map(renderLine);
  const auditBits = state.audit
    ? el("p", { class: "muted" }, `${event.id} · recorded ${formatStamp(event.recordedAt)}${event.reason ? ` · ${event.reason}` : ""}`)
    : null;
  return el("div", { class: history ? "history" : "" }, head, auditBits, ...lines);
}

function renderJournal(): HTMLElement {
  const cards: HTMLElement[] = [];
  for (const group of groupsOf(visibleEvents())) {
    const shown = state.audit ? group.slice().sort((left, right) => left.recordedAt < right.recordedAt ? -1 : 1) : group.filter(isActive);
    if (shown.length === 0) {
      continue;
    }
    const anchor = shown.find(isActive) ?? shown[0];
    const id = anchor.observationIds[0] ? `lineage-${anchor.observationIds.slice().sort()[0]}` : `opening-${anchor.id}`;
    const blocks = shown.map((event, index) => {
      const block = renderEventBlock(event, !isActive(event));
      if (state.audit && shown.length > 1) {
        block.prepend(el("p", { class: "kicker" }, `${index + 1} of ${shown.length}`));
      }
      return block;
    });
    const form = state.openCorrectionId && shown.some((event) => event.id === state.openCorrectionId)
      ? renderCorrectionForm(shown.find((event) => event.id === state.openCorrectionId) as JournalEvent)
      : null;
    const facts = sourceFacts(anchor);
    cards.push(el("article", { class: "entry", id }, ...blocks, form, facts));
  }
  const unposted = ledger.unposted.filter((item) => {
    const observation = observationById(item.observationId);
    if (!observation || state.entityId === "all") {
      return observation !== null;
    }
    const account = ledger.accounts.find((candidate) => candidate.name === observation.accountName);
    return account?.entityId === state.entityId;
  });
  const unpostedCard = unposted.length === 0
    ? null
    : el("section", { class: "unposted" }, el("h2", null, "Not in the journal"), ...unposted.map((item) => {
      const observation = observationById(item.observationId);
      const who = observation ? `${observation.occurredOn} ${observation.merchant} ${formatSigned(observation.amountMinor)}` : item.observationId;
      return el("p", null, `${who}. ${item.reason}`);
    }));
  const demo = renderDemo();
  return el("section", null, demo, ...cards, unpostedCard);
}

function renderDemo(): HTMLElement | null {
  const demo = ledger.demonstration;
  if (!demo) {
    return null;
  }
  const phase = demonstrationState(ledger);
  const observation = observationById(demo.observationId);
  const merchant = observation?.merchant ?? "The sample row";
  const imported = observation ? formatMoney(Math.abs(observation.amountMinor)) : "";
  const help = `${merchant} stays ${imported} in the feed. The walk-through appends a reversal and a ${formatMoney(demo.steps[0].amountMinor)} replacement, then a reversal and a ${formatMoney(demo.steps[1].amountMinor)} replacement.`;
  const button = el("button", { id: "demo-correction", type: "button" }, phase === "done" ? "Show the correction lineage" : "Correction of a correction");
  if (phase === "diverged") {
    button.setAttribute("disabled", "disabled");
    button.textContent = "Reload the page to reset this correction";
  }
  button.addEventListener("click", () => {
    if (phase === "ready") {
      applyDemonstration(ledger);
      state.status = "Appended a correction of a correction. Audit is on. The journal shows the latest entry; the lineage is five events.";
    } else if (phase === "done") {
      state.status = "Audit is on. Five events, in recorded order: original, reversal, replacement, reversal, replacement.";
    }
    state.audit = true;
    state.view = "journal";
    state.openCorrectionId = null;
    render();
    document.getElementById(`lineage-${demo.observationId}`)?.scrollIntoView();
  });
  return el("div", { class: "row-between" }, el("p", null, help), button);
}

function renderTAccount(account: LedgerAccount): HTMLElement | null {
  const lines: Array<{ event: JournalEvent; posting: Posting }> = [];
  for (const event of ledger.events) {
    if (state.entityId !== "all" && account.entityId !== state.entityId) {
      return null;
    }
    for (const posting of event.postings) {
      if (posting.accountId !== account.id) {
        continue;
      }
      if (!state.audit && !isActive(event)) {
        continue;
      }
      lines.push({ event, posting });
    }
  }
  const balance = naturalBalance(ledger.events, account, null);
  if (lines.length === 0 && balance === 0) {
    return null;
  }
  lines.sort((left, right) => left.event.effectiveOn < right.event.effectiveOn ? -1 : left.event.effectiveOn > right.event.effectiveOn ? 1 : left.posting.id < right.posting.id ? -1 : 1);
  const debits = lines.filter((line) => line.posting.side === "debit");
  const credits = lines.filter((line) => line.posting.side === "credit");
  const column = (title: string, className: string, items: Array<{ event: JournalEvent; posting: Posting }>) => el(
    "div",
    { class: className },
    el("h3", { class: "kicker" }, title),
    ...items.map((item) => el(
      "div",
      { class: isActive(item.event) ? "t-line" : "t-line history" },
      el("span", null, item.event.effectiveOn),
      el("span", { class: "num" }, formatMoney(item.posting.amountMinor)),
    )),
  );
  const debitTotal = debits.reduce((sum, item) => sum + item.posting.amountMinor, 0);
  const creditTotal = credits.reduce((sum, item) => sum + item.posting.amountMinor, 0);
  return el(
    "article",
    { class: "t" },
    el("header", { class: "t-head" }, el("h2", null, accountPath(ledger.accounts, account)), el("p", null, `${entityName(account.entityId)} · ${account.accountType} · ${account.kind}`)),
    el("div", { class: "t-cols" }, column("Debit", "debit-col", debits), column("Credit", "credit-col", credits)),
    el("footer", { class: "t-foot" }, el("span", { class: "num" }, formatMoney(debitTotal)), el("span", { class: "num" }, formatMoney(creditTotal)), el("strong", { class: "balance" }, `Balance ${balanceLabel(account, balance)}`)),
  );
}

function renderAccounts(): HTMLElement {
  const typeOrder = ["asset", "liability", "equity", "income", "expense"];
  const cards = ledger.accounts
    .slice()
    .sort((left, right) => typeOrder.indexOf(left.accountType) - typeOrder.indexOf(right.accountType) || left.name.localeCompare(right.name))
    .map(renderTAccount)
    .filter((card): card is HTMLElement => card !== null);
  const rows = ledger.accounts
    .filter((account) => state.entityId === "all" || account.entityId === state.entityId)
    .map((account) => ({ account, totals: sideTotals(ledger.events, account.id) }))
    .filter((row) => row.totals.debit !== 0 || row.totals.credit !== 0)
    .sort((left, right) => typeOrder.indexOf(left.account.accountType) - typeOrder.indexOf(right.account.accountType) || accountPath(ledger.accounts, left.account).localeCompare(accountPath(ledger.accounts, right.account)));
  let debit = 0;
  let credit = 0;
  const body = rows.map((row) => {
    debit += row.totals.debit;
    credit += row.totals.credit;
    return el(
      "tr",
      null,
      el("td", null, accountPath(ledger.accounts, row.account)),
      el("td", { class: "muted" }, row.account.accountType),
      el("td", { class: "num" }, row.totals.debit ? formatMoney(row.totals.debit) : ""),
      el("td", { class: "num" }, row.totals.credit ? formatMoney(row.totals.credit) : ""),
    );
  });
  const table = el(
    "table",
    { class: "trial" },
    el("caption", null, "Trial balance from postings alone. Hidden reversals are included, because a reversal and its original net to zero."),
    el("thead", null, el("tr", null, el("th", null, "Account"), el("th", null, "Type"), el("th", null, "Debit"), el("th", null, "Credit"))),
    el("tbody", null, ...body, el("tr", null, el("th", null, "Total"), el("th", null, ""), el("th", { class: "num" }, formatMoney(debit)), el("th", { class: "num" }, formatMoney(credit)))),
  );
  return el("section", null, el("div", { class: "t-grid" }, ...cards), table);
}

function renderFlight(title: string, lines: ReturnType<typeof reconcileAccount> extends infer _ ? NonNullable<ReturnType<typeof reconcileAccount>>["bookNotStatement"] : never): HTMLElement {
  const body = lines.length === 0
    ? el("p", { class: "muted" }, "None.")
    : el(
      "table",
      { class: "flight" },
      el("thead", null, el("tr", null, el("th", null, "Occurred"), el("th", null, "Posted"), el("th", null, "Memo"), el("th", null, "Effect"))),
      el("tbody", null, ...lines.map((line) => el(
        "tr",
        null,
        el("td", null, line.occurredOn),
        el("td", null, line.postedOn ?? "unknown"),
        el("td", null, line.memo),
        el("td", { class: "num" }, formatSigned(line.naturalMinor)),
      ))),
    );
  return el("section", null, el("h3", null, title), body);
}

function paintDifference(explained: number | null): void {
  const node = document.getElementById("difference");
  if (!node) {
    return;
  }
  node.classList.remove("agreed", "out");
  if (explained === null) {
    node.textContent = "Choose an account.";
    return;
  }
  if (state.endingRaw.trim() === "") {
    node.textContent = "Type a statement ending.";
    return;
  }
  let typed: number;
  try {
    typed = parseSignedMinor(state.endingRaw);
  } catch {
    node.textContent = "The ending must be a decimal amount.";
    node.classList.add("out");
    return;
  }
  const difference = typed - explained;
  node.textContent = difference === 0 ? `${formatMoney(0)} Agreed.` : `${formatSigned(difference)}  Statement ending − explained balance.`;
  node.classList.add(difference === 0 ? "agreed" : "out");
}

function renderReconcile(): HTMLElement {
  const financial = ledger.accounts.filter(isFinancial).slice().sort((left, right) => left.name.localeCompare(right.name));
  const select = el("select", { id: "recon-account" }, ...financial.map((account) => {
    const option = el("option", { value: account.id }, account.name);
    if (account.id === state.accountId) {
      option.setAttribute("selected", "selected");
    }
    return option;
  }));
  select.addEventListener("change", () => {
    if (select instanceof HTMLSelectElement) {
      state.accountId = select.value;
      render();
    }
  });
  const cutoff = el("input", { id: "cutoff", type: "date", value: state.cutoff });
  cutoff.addEventListener("change", () => {
    if (cutoff instanceof HTMLInputElement && cutoff.value) {
      state.cutoff = cutoff.value;
      render();
    }
  });
  const sheet = reconcileAccount(ledger, state.accountId, state.cutoff);
  const sense = sheet?.account.accountType === "liability"
    ? "A credit balance is the amount owed."
    : "A debit balance is the amount on hand.";
  const hint = sheet
    ? ledger.statements.find((item) => item.accountName === sheet.account.name && item.cutoff === state.cutoff) ?? null
    : null;
  const ending = el("input", { id: "ending", value: state.endingRaw, inputmode: "decimal", autocomplete: "off", spellcheck: "false" });
  ending.addEventListener("input", () => {
    if (ending instanceof HTMLInputElement) {
      state.endingRaw = ending.value;
      paintDifference(sheet ? sheet.explainedMinor : null);
    }
  });
  const difference = el("p", { id: "difference" }, "");
  const unposted = sheet && sheet.unposted.length > 0
    ? el("section", null, el("h3", null, "Observed on this account, not posted"), el("p", { class: "muted" }, "These rows are outside the formula. A transfer with no counterpart is not an in-flight item."), ...sheet.unposted.map((observation) => {
      const reason = ledger.unposted.find((item) => item.observationId === observation.id)?.reason ?? "";
      return el("p", null, `${observation.occurredOn} ${observation.merchant} ${formatSigned(observation.amountMinor)}. ${reason}`);
    }))
    : null;
  const unknown = sheet && sheet.unknownPosted.length > 0
    ? el("section", null, el("h3", null, "Posted date unknown"), el("p", { class: "muted" }, "These lines stay in the occurred balance and out of the in-flight lists. The feed has no posted date for them."), ...sheet.unknownPosted.map((line) => el("p", null, `${line.occurredOn} ${line.memo} ${formatSigned(line.naturalMinor)}`)))
    : null;
  const worksheet = sheet
    ? el(
      "table",
      { class: "sheet" },
      el("tbody", null,
        el("tr", null, el("th", null, "Occurred balance"), el("td", { class: "num" }, balanceLabel(sheet.account, sheet.occurredMinor))),
        el("tr", null, el("th", null, "Less occurred on or before, posted after"), el("td", { class: "num" }, formatSigned(-sheet.bookNotStatement.reduce((sum, line) => sum + line.naturalMinor, 0)))),
        el("tr", null, el("th", null, "Plus posted on or before, occurred after"), el("td", { class: "num" }, formatSigned(sheet.statementNotBook.reduce((sum, line) => sum + line.naturalMinor, 0)))),
        el("tr", { class: "total" }, el("th", null, "Explained statement balance"), el("td", { class: "num" }, balanceLabel(sheet.account, sheet.explainedMinor))),
      ),
    )
    : el("p", null, "This ledger has no financial account to reconcile.");
  queueMicrotask(() => paintDifference(sheet ? sheet.explainedMinor : null));
  return el(
    "section",
    { class: "sheet-wrap" },
    el("p", null, "The occurred date assigns the period. Posted date only explains the gap to a statement: occurred balance, minus items that occurred on or before the cutoff but posted after, plus items that posted on or before the cutoff but occurred after."),
    el("div", { class: "ending" }, el("label", null, "Account", select), el("label", null, "Cutoff", cutoff)),
    el("p", { class: "muted" }, sense),
    el("p", { class: "muted" }, "In-flight lines are the current journal. The occurred balance is the sum of every posting, so a reversal and its original net to zero."),
    worksheet,
    sheet ? renderFlight("Subtract — occurred on or before the cutoff, posted after", sheet.bookNotStatement) : null,
    sheet ? renderFlight("Add — posted on or before the cutoff, occurred after", sheet.statementNotBook) : null,
    unknown,
    unposted,
    hint ? el("p", null, `${hint.note} ${formatMoney(hint.endingMinor)}. Type it; the field is not filled in for you.`) : null,
    el("label", null, "Statement ending", ending),
    el("h3", null, "Difference"),
    difference,
  );
}

function booksLine(): HTMLElement {
  const totals = trialTotals(ledger.events);
  const balanced = totals.debit === totals.credit && supersedeProblems(ledger.events).length === 0;
  const eqs = equations(ledger.events, ledger.accounts, ledger.entities.map((entity) => entity.id));
  const text = eqs.map((equation) => {
    const name = entityName(equation.entityId);
    return `${name}: assets ${formatMoney(equation.assets)} = liabilities ${formatMoney(equation.liabilities)} + equity ${formatMoney(equation.equity)} + income ${formatMoney(equation.income)} − expenses ${formatMoney(equation.expenses)}`;
  }).join(" · ");
  const ok = balanced && eqs.every((equation) => equation.discrepancy === 0);
  return el(
    "p",
    { id: "books-ok", class: ok ? "equations agreed" : "equations out" },
    ok
      ? `Debits ${formatMoney(totals.debit)} = credits ${formatMoney(totals.credit)}. ${text}`
      : "Books do not balance.",
  );
}

function render(): void {
  const root = document.getElementById("app");
  if (!root) {
    return;
  }
  const source = ledger.source === "fixture" ? "Synthetic fixture" : "YA_MONARCH_CSV";
  const audit = el("button", { id: "audit-toggle", type: "button", "aria-pressed": state.audit ? "true" : "false" }, state.audit ? "Audit on" : "Audit off");
  audit.addEventListener("click", () => {
    state.audit = !state.audit;
    render();
  });
  const views: Array<[ViewName, string]> = [["journal", "Journal"], ["accounts", "T-accounts"], ["reconcile", "Reconciliation"]];
  const nav = views.map(([name, label]) => {
    const button = el("button", { type: "button" }, label);
    if (state.view === name) {
      button.setAttribute("aria-current", "page");
    }
    button.addEventListener("click", () => {
      state.view = name;
      state.openCorrectionId = null;
      render();
    });
    return button;
  });
  const entity = el("select", { id: "entity-filter" }, el("option", { value: "all" }, "All entities"), ...ledger.entities.map((item) => {
    const option = el("option", { value: item.id }, item.name);
    if (item.id === state.entityId) {
      option.setAttribute("selected", "selected");
    }
    return option;
  }));
  entity.addEventListener("change", () => {
    if (entity instanceof HTMLSelectElement) {
      state.entityId = entity.value;
      render();
    }
  });
  const body = state.view === "journal" ? renderJournal() : state.view === "accounts" ? renderAccounts() : renderReconcile();
  root.replaceChildren(el(
    "div",
    { class: "shell" },
    el(
      "nav",
      { class: "nav" },
      el("p", { class: "kicker" }, "Prototype C"),
      el("p", null, "Journal first"),
      el("div", { class: "nav-buttons" }, ...nav),
      el("label", null, "Entity", entity),
      el("p", { class: "muted" }, "Debits on the left, credits on the right. Reload discards corrections. Nothing is stored in the browser."),
    ),
    el(
      "main",
      { class: "main" },
      el("header", { class: "mast" }, el("div", null, el("p", { class: "kicker" }, source), el("h1", null, "Journal"), booksLine()), audit),
      el("p", { id: "status", "aria-live": "polite" }, state.status),
      el("p", { class: "muted" }, "Monarch Reviewed and Pending are kept on the source row and are not the journal. Categories are income and expense accounts."),
      body,
    ),
  ));
  if (state.openCorrectionId) {
    const correctionId = state.openCorrectionId;
    queueMicrotask(() => paintPreview(correctionId));
  }
}

async function boot(): Promise<void> {
  let response: Response;
  try {
    response = await fetch("/api/source");
  } catch {
    showFatal("The feed could not be loaded.");
    return;
  }
  if (!response.ok) {
    showFatal("The feed could not be loaded.");
    return;
  }
  let payload: unknown;
  try {
    payload = await response.json();
  } catch {
    showFatal("The feed could not be loaded.");
    return;
  }
  try {
    ledger = ledgerFromPayload(payload);
  } catch (caught) {
    showFatal(caught instanceof FeedError ? caught.message : "The feed could not be loaded.");
    return;
  }
  const financial = ledger.accounts.filter(isFinancial);
  const hinted = ledger.statements[0]
    ? financial.find((account) => account.name === ledger.statements[0].accountName)
    : null;
  state.accountId = hinted?.id ?? financial[0]?.id ?? "";
  state.cutoff = ledger.defaultCutoff;
  render();
}

boot();
