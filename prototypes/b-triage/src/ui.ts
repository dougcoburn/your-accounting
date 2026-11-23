import {
  applyTreatment,
  booksFromObservations,
  inboxLines,
  progress,
  skipToWorklist,
  voidLine,
  worklistLines,
} from "./ledger.ts";
import { formatSignedUsd, formatUsd } from "./money.ts";
import { suggestAll } from "./suggestions.ts";
import type { Books, Line, Observation, SettlementGroupSuggestion, Treatment } from "./types.ts";

type View = "inbox" | "worklist" | "ledger";
type Bootstrap = {
  source: "synthetic" | "env";
  count: number;
  issueCount: number;
  honorMonarchReviewed?: boolean;
  unreviewedCount?: number;
  observations: Observation[];
};

let loadedObservations: Observation[] = [];
let honorMonarchReviewed = false;
let initialBooks: Books;
let books: Books;
let treatments: Map<string, Treatment>;
let view: View = "inbox";
let selectedKey = "";
let flash = "";
let sourceLabel = "";

const app = document.getElementById("app")!;

function treatmentsFor(observations: Observation[]): Map<string, Treatment> {
  return suggestAll(observations);
}

function observedId(line: Line): string | undefined {
  return line.observation?.id;
}

function setFlash(message: string): void {
  flash = message;
}

function go(next: Books, message: string): void {
  books = next;
  setFlash(message);
  const inbox = inboxLines(books);
  if (!inbox.some((l) => l.key === selectedKey)) {
    selectedKey = inbox[0]?.key ?? "";
  }
  render();
}

function applySelected(): void {
  const line = books.lines.find((l) => l.key === selectedKey);
  const id = observedId(line ?? ({} as Line));
  if (!id) return;
  const treatment = treatments.get(id);
  if (!treatment) return;
  const result = applyTreatment(books, id, treatment);
  if (!result.ok) {
    setFlash(result.reason || "Could not apply");
    render();
    return;
  }
  go(result.books, `Applied ${treatment.recipeLabel} to this line.`);
}

function skipSelected(): void {
  const line = books.lines.find((l) => l.key === selectedKey);
  const id = observedId(line ?? ({} as Line));
  if (!id) return;
  const result = skipToWorklist(books, id);
  if (!result.ok) {
    setFlash(result.reason || "Could not skip");
    render();
    return;
  }
  go(result.books, "Deferred to the worklist as open.");
}

function voidSelected(): void {
  const line = books.lines.find((l) => l.key === selectedKey);
  const id = observedId(line ?? ({} as Line));
  if (!id) return;
  const result = voidLine(books, id);
  if (!result.ok) {
    setFlash(result.reason || "Could not void");
    render();
    return;
  }
  go(result.books, "Voided. The observation is tombstoned in this toy ledger.");
}

function cloneBooks(source: Books): Books {
  return {
    lines: source.lines.map((l) => ({ ...l })),
    entries: source.entries.map((e) => ({ ...e, postings: e.postings.map((p) => ({ ...p })) })),
    entrySeq: source.entrySeq,
    expectedSeq: source.expectedSeq,
    groupSeq: source.groupSeq,
  };
}

function loadBooks(): void {
  initialBooks = booksFromObservations(loadedObservations, { honorMonarchReviewed });
  books = cloneBooks(initialBooks);
  view = "inbox";
  selectedKey = inboxLines(books)[0]?.key ?? "";
}

function reset(): void {
  loadBooks();
  setFlash("In-memory state reset. Nothing was stored in the browser.");
  render();
}

function setHonorMonarchReviewed(next: boolean): void {
  honorMonarchReviewed = next;
  loadBooks();
  setFlash(
    next
      ? "Honoring Monarch Reviewed. Those rows start reconciled. This setting stays in memory only."
      : "Ignoring Monarch Reviewed. Every row starts unreviewed. This setting stays in memory only.",
  );
  render();
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function chips(obs: Observation): string {
  const parts = [
    obs.pending ? `<span class="chip pending">Pending</span>` : "",
    obs.reviewed ? `<span class="chip monarch">Monarch: reviewed</span>` : "",
    obs.category ? `<span class="chip">${escapeHtml(obs.category)}</span>` : `<span class="chip warn">No category</span>`,
    `<span class="chip">${escapeHtml(obs.account)}</span>`,
    obs.tags ? `<span class="chip">${escapeHtml(obs.tags)}</span>` : "",
  ];
  return `<div class="chips">${parts.join("")}</div>`;
}

function groupHtml(group: SettlementGroupSuggestion): string {
  const rows = group.participants
    .map((p) => {
      const mark = p.filled ? "✓" : "○";
      const markClass = p.filled ? "mark" : "mark empty";
      const unfilled = p.filled ? "" : " unfilled";
      const origin = p.origin === "expected" ? `<span class="chip expected">expected</span> ` : "";
      return `<div class="participant${unfilled}">
        <span class="${markClass}" aria-hidden="true">${mark}</span>
        <div class="label">${origin}${escapeHtml(p.label)}</div>
        <div class="money">${escapeHtml(formatSignedUsd(p.amountMinor))}</div>
      </div>`;
    })
    .join("");
  const note = group.note ? `<p class="group-note">${escapeHtml(group.note)}</p>` : "";
  const status = group.complete ? "complete" : "incomplete";
  return `<div class="group">
    <h3>Settlement group · ${escapeHtml(group.title)} · ${status}</h3>
    ${rows}
    ${note}
  </div>`;
}

function cardHtml(line: Line, selected: boolean): string {
  const obs = line.observation;
  if (!obs) return "";
  const treatment = treatments.get(obs.id);
  const pendingClass = obs.pending ? " pending" : "";
  const amountClass = obs.amountMinor >= 0 ? "in" : "out";
  const selectedAttr = selected ? ' aria-current="true" style="outline: 2px solid var(--pine);"' : "";
  const group = treatment?.settlementGroup ? groupHtml(treatment.settlementGroup) : "";
  return `<article class="card${pendingClass}" data-key="${escapeHtml(line.key)}"${selectedAttr}>
    <div class="card-top">
      <span>${escapeHtml(obs.date)}${obs.pending ? " · not yet posted" : ""}</span>
      <span>${escapeHtml(obs.owner || "")}</span>
    </div>
    <div class="merchant-row">
      <h2>${escapeHtml(obs.merchant || "(no merchant)")}</h2>
      <div class="amount ${amountClass}">${escapeHtml(formatSignedUsd(obs.amountMinor))}</div>
    </div>
    ${chips(obs)}
    ${obs.originalStatement ? `<p class="statement">Original statement: ${escapeHtml(obs.originalStatement)}</p>` : ""}
    ${obs.notes ? `<p class="statement">Notes: ${escapeHtml(obs.notes)}</p>` : ""}
    <div class="treatment">
      <h3>Suggested treatment</h3>
      <div class="treatment-row">
        <p><strong>${escapeHtml(treatment?.recipeLabel ?? "—")}</strong>${
          treatment?.category ? ` · ${escapeHtml(treatment.category)}` : ""
        }<br /><span class="hint">${escapeHtml(treatment?.summary ?? "")}</span></p>
        <button class="btn primary" data-act="apply" data-key="${escapeHtml(line.key)}">Apply</button>
      </div>
    </div>
    ${group}
    <div class="actions">
      <button class="btn ghost" data-act="skip" data-key="${escapeHtml(line.key)}">Skip / defer to worklist</button>
      <button class="btn danger" data-act="void" data-key="${escapeHtml(line.key)}">Void</button>
    </div>
  </article>`;
}

function workItemHtml(line: Line): string {
  if (line.origin === "expected" && line.expected) {
    const e = line.expected;
    return `<div class="work-item">
      <div class="chips"><span class="chip expected">expected</span><span class="chip">${escapeHtml(line.state)}</span></div>
      <div class="merchant-row">
        <h2 style="font-size:1.05rem">${escapeHtml(e.label)}</h2>
        <div class="amount in">${escapeHtml(formatSignedUsd(e.amountMinor))}</div>
      </div>
      <p class="statement">${escapeHtml(e.account)} · related observation ${escapeHtml(e.relatedObservationId)} · ${escapeHtml(e.date)}</p>
      <p class="group-note">This slot can sit unfilled. It is its own draft settlement group, not a committed clearing balance.</p>
    </div>`;
  }
  const obs = line.observation;
  if (!obs) return "";
  return `<div class="work-item">
    <div class="chips"><span class="chip">${escapeHtml(line.state)}</span>${
      line.appliedRecipe ? `<span class="chip">${escapeHtml(line.appliedRecipe)}</span>` : ""
    }</div>
    <div class="merchant-row">
      <h2 style="font-size:1.05rem">${escapeHtml(obs.merchant)}</h2>
      <div class="amount">${escapeHtml(formatSignedUsd(obs.amountMinor))}</div>
    </div>
    <p class="statement">${escapeHtml(obs.date)} · ${escapeHtml(obs.account)}</p>
  </div>`;
}

function ledgerHtml(): string {
  if (books.entries.length === 0) {
    return `<p class="empty">No journal entries yet. Apply a treatment from the inbox.</p>`;
  }
  const rows = [...books.entries].reverse().map((entry) => {
    const postings = entry.postings
      .map((p) => {
        const debit = p.debitMinor ? formatUsd(p.debitMinor) : "";
        const credit = p.creditMinor ? formatUsd(p.creditMinor) : "";
        return `<tr>
          <td></td><td></td>
          <td>${escapeHtml(p.account)}</td>
          <td class="money">${escapeHtml(debit)}</td>
          <td class="money">${escapeHtml(credit)}</td>
        </tr>`;
      })
      .join("");
    return `<tr>
      <td>${escapeHtml(entry.date)}<br /><span class="hint">${escapeHtml(entry.id)}</span></td>
      <td>${escapeHtml(entry.recipeLabel)}<br /><span class="hint">${escapeHtml(entry.settlementGroupId)}</span></td>
      <td colspan="3">${escapeHtml(entry.memo)}</td>
    </tr>${postings}`;
  });
  return `<table class="ledger">
    <thead><tr><th>Date</th><th>Recipe</th><th>Account / memo</th><th class="money">Debit</th><th class="money">Credit</th></tr></thead>
    <tbody>${rows.join("")}</tbody>
  </table>`;
}

function render(): void {
  const p = progress(books);
  const pct = p.total === 0 ? 0 : Math.round((p.reached / p.total) * 100);
  const inbox = inboxLines(books);
  const work = worklistLines(books);
  if (!selectedKey && inbox[0]) selectedKey = inbox[0].key;

  let body = "";
  if (view === "inbox") {
    body =
      inbox.length === 0
        ? `<p class="empty">Inbox is clear. Every unreviewed observation has been applied, deferred, or voided.</p>`
        : `<div class="cards">${inbox.map((l) => cardHtml(l, l.key === selectedKey)).join("")}</div>`;
  } else if (view === "worklist") {
    body =
      work.length === 0
        ? `<p class="empty">Worklist is empty. Skipped lines, incomplete groups, and unfilled expected slots will land here.</p>`
        : work.map(workItemHtml).join("");
  } else {
    body = ledgerHtml();
  }

  const flashHtml = flash ? `<div class="flash">${escapeHtml(flash)}</div>` : "";

  app.innerHTML = `
    <div class="shell">
      <header class="top">
        <div class="brand">
          <h1>Inbox triage</h1>
          <div class="meta">Prototype B · ${escapeHtml(sourceLabel)} · ${p.total} observations · state is in memory only</div>
        </div>
        <div class="progress-block">
          <div class="progress-label">
            <span><strong>${p.reached} of ${p.total}</strong> reconciled, voided, or open</span>
            <span>${pct}%</span>
          </div>
          <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="${p.total}" aria-valuenow="${p.reached}">
            <span style="width:${pct}%"></span>
          </div>
          <div class="counts">
            <span>Reconciled <b>${p.reconciled}</b></span>
            <span>Voided <b>${p.voided}</b></span>
            <span>Open <b>${p.open}</b></span>
            <span>Inbox / unreviewed <b>${p.unreviewed}</b></span>
            <span>Interpreted <b>${p.interpreted}</b></span>
          </div>
        </div>
        <nav class="tabs" aria-label="Views">
          <button data-view="inbox" aria-current="${view === "inbox" ? "page" : "false"}">Inbox (${p.unreviewed})</button>
          <button data-view="worklist" aria-current="${view === "worklist" ? "page" : "false"}">Worklist (${work.length})</button>
          <button data-view="ledger" aria-current="${view === "ledger" ? "page" : "false"}">Toy ledger (${books.entries.length})</button>
        </nav>
      </header>
      <div class="toolbar">
        <p class="hint"><kbd>J</kbd>/<kbd>K</kbd> move · <kbd>Enter</kbd> apply · <kbd>S</kbd> skip · <kbd>V</kbd> void. Refresh or Reset clears in-memory state. No browser storage, no network egress.</p>
        <div class="toolbar-actions">
          <label class="toggle">
            <input type="checkbox" data-honor-reviewed ${honorMonarchReviewed ? "checked" : ""} />
            Honor Monarch Reviewed
          </label>
          <button class="btn ghost" data-act="reset">Reset</button>
        </div>
      </div>
      ${flashHtml}
      ${body}
    </div>
  `;

  app.querySelectorAll("[data-view]").forEach((el) => {
    el.addEventListener("click", () => {
      view = (el.getAttribute("data-view") || "inbox") as View;
      flash = "";
      render();
    });
  });
  app.querySelectorAll("[data-act]").forEach((el) => {
    el.addEventListener("click", () => {
      const act = el.getAttribute("data-act");
      const key = el.getAttribute("data-key");
      if (key) selectedKey = key;
      if (act === "apply") applySelected();
      else if (act === "skip") skipSelected();
      else if (act === "void") voidSelected();
      else if (act === "reset") reset();
    });
  });
  const honorBox = app.querySelector<HTMLInputElement>("[data-honor-reviewed]");
  honorBox?.addEventListener("change", () => {
    setHonorMonarchReviewed(honorBox.checked);
  });
  app.querySelectorAll("article.card").forEach((el) => {
    el.addEventListener("click", () => {
      const key = el.getAttribute("data-key");
      if (key && key !== selectedKey) {
        selectedKey = key;
        flash = "";
        render();
        const node = app.querySelector(`article.card[data-key="${CSS.escape(key)}"]`);
        node?.scrollIntoView({ block: "nearest" });
      }
    });
  });
}

function moveSelection(delta: number): void {
  const inbox = inboxLines(books);
  if (inbox.length === 0) return;
  const index = Math.max(0, inbox.findIndex((l) => l.key === selectedKey));
  const next = inbox[(index + delta + inbox.length) % inbox.length]!;
  selectedKey = next.key;
  flash = "";
  view = "inbox";
  render();
  const node = app.querySelector(`article.card[data-key="${CSS.escape(next.key)}"]`);
  node?.scrollIntoView({ block: "nearest" });
}

window.addEventListener("keydown", (event) => {
  const target = event.target as HTMLElement | null;
  if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
  if (event.key === "j" || event.key === "J" || event.key === "ArrowDown") {
    event.preventDefault();
    moveSelection(1);
  } else if (event.key === "k" || event.key === "K" || event.key === "ArrowUp") {
    event.preventDefault();
    moveSelection(-1);
  } else if (event.key === "Enter") {
    if (view === "inbox") {
      event.preventDefault();
      applySelected();
    }
  } else if (event.key === "s" || event.key === "S") {
    if (view === "inbox") {
      event.preventDefault();
      skipSelected();
    }
  } else if (event.key === "v" || event.key === "V") {
    if (view === "inbox") {
      event.preventDefault();
      voidSelected();
    }
  }
});

async function start(): Promise<void> {
  try {
    const res = await fetch("/api/bootstrap");
    if (!res.ok) throw new Error("bootstrap failed");
    const data = (await res.json()) as Bootstrap;
    sourceLabel = data.source === "env" ? "YA_MONARCH_CSV" : "synthetic fixture";
    loadedObservations = data.observations;
    honorMonarchReviewed = data.honorMonarchReviewed === true;
    treatments = treatmentsFor(loadedObservations);
    loadBooks();
    render();
  } catch {
    app.innerHTML = `<p class="error">Could not load observations from the local server.</p>`;
  }
}

void start();
