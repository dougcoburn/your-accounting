import { formatCents } from "/lib/money.mjs";
import {
  CATEGORIES,
  RECIPES,
  getCategory,
  getRecipe,
  nextReviewState,
  previewRecipe,
  suggestCategoryFromHint,
} from "/lib/recipes.mjs";

const COLUMNS = [
  "occurred",
  "origin",
  "last4",
  "merchant",
  "amount",
  "hint",
  "recipe",
  "category",
  "state",
];

/** @type {import("../lib/adapter.mjs").RegisterLine[]} */
let lines = [];
let source = "synthetic";
let observationCount = 0;
let expectedCount = 0;

/** Visible (filtered) line ids, in display order. */
let visibleIds = [];
/** Index into visibleIds. */
let cursor = 0;
let focusCol = "recipe";
/** @type {Set<string>} */
let selectedIds = new Set();
let originFilter = "all";
let /** @type {Set<string>} */ stateFilter = new Set();
let filterText = "";
let statusText = "";
let helpOpen = false;

/**
 * @typedef {object} EditState
 * @property {string} lineId
 * @property {"recipe" | "category" | "counter"} field
 * @property {string} query
 * @property {number} active
 * @property {string[]} options
 */

/** @type {EditState | null} */
let editing = null;

const els = {
  grid: document.getElementById("grid"),
  body: document.getElementById("grid-body"),
  filter: document.getElementById("filter-input"),
  origin: document.getElementById("origin-filters"),
  state: document.getElementById("state-filters"),
  counts: document.getElementById("counts"),
  meta: document.getElementById("source-meta"),
  preview: document.getElementById("preview-body"),
  status: document.getElementById("status"),
  help: document.getElementById("help"),
  helpClose: document.getElementById("help-close"),
  comboRoot: document.getElementById("combo-root"),
};

function byId(id) {
  return lines.find((line) => line.id === id);
}

function financialAccounts() {
  const seen = new Map();
  for (const line of lines) {
    if (line.account && !seen.has(line.account)) seen.set(line.account, line.account);
  }
  return [...seen.values()];
}

function visibleLines() {
  const q = filterText.trim().toLowerCase();
  return lines.filter((line) => {
    if (originFilter !== "all" && line.origin !== originFilter) return false;
    if (stateFilter.size > 0 && !stateFilter.has(line.state)) return false;
    if (!q) return true;
    const recipe = line.recipeId ? getRecipe(line.recipeId) : undefined;
    const category = line.categoryId ? getCategory(line.categoryId) : undefined;
    const hay = [
      line.merchant,
      line.account,
      line.accountLast4,
      line.monarchCategory,
      line.notes,
      line.originalStatement,
      line.entityHint,
      recipe?.name,
      category?.name,
      line.origin,
      line.state,
    ]
      .join(" ")
      .toLowerCase();
    return hay.includes(q);
  });
}

function setStatus(message) {
  statusText = message;
  els.status.textContent = message;
}

function renderMeta() {
  const dates = lines.map((line) => line.occurredDate).filter(Boolean).sort();
  const range = dates.length ? `${dates[0]} – ${dates[dates.length - 1]}` : "no dates";
  const sourceLabel = source === "YA_MONARCH_CSV" ? "YA_MONARCH_CSV" : "synthetic fixture";
  els.meta.innerHTML = "";
  const sourceBadge = document.createElement("span");
  sourceBadge.className = "badge";
  sourceBadge.textContent = sourceLabel;
  const rangeBadge = document.createElement("span");
  rangeBadge.className = "badge";
  rangeBadge.textContent = range;
  const countBadge = document.createElement("span");
  countBadge.className = "badge";
  countBadge.textContent = `${observationCount} observed · ${expectedCount} expected`;
  els.meta.append(sourceBadge, rangeBadge, countBadge);
}

function renderChips() {
  els.origin.replaceChildren();
  const originLegend = document.createElement("legend");
  originLegend.textContent = "Origin";
  els.origin.append(originLegend);
  for (const [value, label] of [
    ["all", "All"],
    ["observed", "Observed"],
    ["expected", "Expected"],
  ]) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "chip";
    btn.textContent = label;
    btn.setAttribute("aria-pressed", originFilter === value ? "true" : "false");
    btn.addEventListener("click", () => {
      originFilter = value;
      refresh({ keepStatus: true });
    });
    els.origin.append(btn);
  }

  els.state.replaceChildren();
  const stateLegend = document.createElement("legend");
  stateLegend.textContent = "Review state";
  els.state.append(stateLegend);
  const all = document.createElement("button");
  all.type = "button";
  all.className = "chip";
  all.textContent = "All";
  all.setAttribute("aria-pressed", stateFilter.size === 0 ? "true" : "false");
  all.addEventListener("click", () => {
    stateFilter = new Set();
    refresh({ keepStatus: true });
  });
  els.state.append(all);
  for (const state of ["unreviewed", "interpreted", "reconciled", "voided"]) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "chip";
    btn.textContent = state;
    btn.setAttribute("aria-pressed", stateFilter.has(state) ? "true" : "false");
    btn.addEventListener("click", () => {
      if (stateFilter.has(state)) stateFilter.delete(state);
      else stateFilter.add(state);
      refresh({ keepStatus: true });
    });
    els.state.append(btn);
  }
}

function renderCounts(visible) {
  const tally = { unreviewed: 0, interpreted: 0, reconciled: 0, voided: 0 };
  for (const line of lines) tally[line.state] += 1;
  els.counts.textContent = `${visible.length} shown of ${lines.length} · ${tally.unreviewed} unreviewed · ${tally.interpreted} interpreted · ${tally.reconciled} reconciled · ${tally.voided} voided`;
}

function cellText(line, col) {
  switch (col) {
    case "occurred":
      return line.occurredDate;
    case "origin":
      return line.origin === "expected" ? "Expected" : "Observed";
    case "last4":
      return line.accountLast4 ? `••••${line.accountLast4}` : "—";
    case "merchant":
      return line.merchant;
    case "amount":
      return formatCents(line.amountMinor);
    case "hint":
      return line.monarchCategory || "—";
    case "recipe":
      return line.recipeId ? getRecipe(line.recipeId)?.name || line.recipeId : "Choose recipe";
    case "category": {
      const recipe = line.recipeId ? getRecipe(line.recipeId) : undefined;
      if (recipe?.needsCounterAccount) {
        return line.counterAccount || "Choose account";
      }
      if (line.categoryId) return getCategory(line.categoryId)?.name || line.categoryId;
      if (recipe && !recipe.needsCategory) return "—";
      return "Choose category";
    }
    case "state":
      return line.state;
    default:
      return "";
  }
}

function renderGrid(visible) {
  visibleIds = visible.map((line) => line.id);
  if (cursor >= visibleIds.length) cursor = Math.max(0, visibleIds.length - 1);
  if (cursor < 0) cursor = 0;
  const cursorId = visibleIds[cursor];
  if (cursorId && selectedIds.size === 0) selectedIds.add(cursorId);

  els.grid.setAttribute("aria-rowcount", String(visible.length + 1));
  els.body.replaceChildren();

  visible.forEach((line, index) => {
    const tr = document.createElement("tr");
    tr.role = "row";
    tr.dataset.id = line.id;
    tr.setAttribute("aria-rowindex", String(index + 2));
    const selected = selectedIds.has(line.id);
    tr.setAttribute("aria-selected", selected ? "true" : "false");
    if (line.state === "voided") tr.classList.add("voided");

    COLUMNS.forEach((col, colIndex) => {
      const td = document.createElement("td");
      td.role = "gridcell";
      td.dataset.col = col;
      td.setAttribute("aria-colindex", String(colIndex + 1));
      const isCursor = line.id === cursorId && col === focusCol;
      td.tabIndex = isCursor ? 0 : -1;
      if (isCursor) td.classList.add("focused");

      if (col === "amount") {
        td.classList.add("num");
        const span = document.createElement("span");
        span.className = `amount ${line.amountMinor < 0 ? "out" : "in"}`;
        span.textContent = formatCents(line.amountMinor);
        td.append(span);
      } else if (col === "origin") {
        td.className = `origin ${line.origin}`;
        td.textContent = cellText(line, col);
      } else if (col === "last4") {
        td.className = "last4";
        td.textContent = cellText(line, col);
        td.title = line.account;
      } else if (col === "merchant") {
        const wrap = document.createElement("div");
        wrap.className = "merchant";
        const name = document.createElement("span");
        name.className = "merchant-name";
        name.textContent = line.merchant;
        name.title = line.notes || line.originalStatement || line.merchant;
        wrap.append(name);
        if (line.pending) {
          const pending = document.createElement("span");
          pending.className = "pending";
          pending.textContent = "Pending";
          wrap.append(pending);
        }
        td.append(wrap);
      } else if (col === "recipe" || col === "category" || col === "hint") {
        const empty =
          (col === "recipe" && !line.recipeId) ||
          (col === "category" && !line.categoryId && !line.counterAccount);
        td.className = empty ? "placeholder" : "";
        td.textContent = cellText(line, col);
      } else if (col === "state") {
        td.className = `state ${line.state}`;
        td.textContent = line.state;
      } else {
        td.textContent = cellText(line, col);
      }

      td.addEventListener("mousedown", (event) => {
        event.preventDefault();
        const extend = event.shiftKey;
        moveCursorTo(index, col, extend);
        refresh();
        if (event.detail === 2 && (col === "recipe" || col === "category")) {
          startEdit(col === "category" && getRecipe(line.recipeId)?.needsCounterAccount ? "counter" : col === "category" ? "category" : "recipe");
        }
      });
      tr.append(td);
    });
    els.body.append(tr);
  });

  const focused = els.body.querySelector("td.focused");
  if (focused && document.activeElement !== els.filter && !editing && !helpOpen) {
    focused.focus({ preventScroll: true });
    focused.scrollIntoView({ block: "nearest" });
  }
}

/**
 * @param {import("../lib/adapter.mjs").RegisterLine} line
 */
function linePreview(line) {
  return previewRecipe({
    account: line.account,
    amountMinor: line.amountMinor,
    merchant: line.merchant,
    notes: line.notes,
    recipeId: line.recipeId || undefined,
    categoryId: line.categoryId || undefined,
    counterAccount: line.counterAccount || undefined,
  });
}

function renderPreview() {
  const ids = visibleIds.filter((id) => selectedIds.has(id));
  const selected = ids.map(byId).filter(Boolean);
  els.preview.replaceChildren();
  if (selected.length === 0) {
    const p = document.createElement("p");
    p.className = "muted";
    p.textContent = "Select a row to preview postings for its recipe.";
    els.preview.append(p);
    return;
  }

  const list = document.createElement("div");
  list.className = "preview-list";
  let debitTotal = 0;
  let creditTotal = 0;

  for (const line of selected) {
    const preview = linePreview(line);
    debitTotal += preview.debits;
    creditTotal += preview.credits;
    const block = document.createElement("section");
    block.className = "preview-head";
    const h3 = document.createElement("h3");
    h3.textContent = `${line.merchant} · ${formatCents(line.amountMinor)}`;
    const sub = document.createElement("p");
    sub.className = "muted";
    const recipe = line.recipeId ? getRecipe(line.recipeId) : undefined;
    const bits = [
      line.occurredDate,
      line.account,
      recipe ? recipe.name : "No recipe",
      line.origin,
    ];
    if (line.entityHint) bits.push(line.entityHint);
    sub.textContent = bits.join(" · ");
    block.append(h3, sub, postingTable(preview, recipe));
    list.append(block);
  }

  if (selected.length > 1) {
    const combined = document.createElement("section");
    const h3 = document.createElement("h3");
    h3.textContent = `Combined (${selected.length} lines)`;
    combined.append(h3, flag(debitTotal, creditTotal));
    list.append(combined);
  }

  els.preview.append(list);
}

function postingTable(preview, recipe) {
  const wrap = document.createElement("div");
  if (!recipe) {
    const p = document.createElement("p");
    p.className = "muted";
    p.textContent = "Choose a recipe to see debit and credit postings.";
    wrap.append(p);
    return wrap;
  }
  if (recipe.needsCategory && preview.postings.length === 0) {
    const p = document.createElement("p");
    p.className = "muted";
    p.textContent = "Choose a leaf category to complete this preview.";
    wrap.append(p);
    return wrap;
  }
  if (recipe.needsCounterAccount && preview.postings.length === 0) {
    const p = document.createElement("p");
    p.className = "muted";
    p.textContent = "Choose the other financial account to complete this preview.";
    wrap.append(p);
    return wrap;
  }

  const table = document.createElement("table");
  table.className = "postings";
  table.innerHTML =
    "<thead><tr><th>Ledger account</th><th class='num'>Debit</th><th class='num'>Credit</th></tr></thead>";
  const tbody = document.createElement("tbody");
  for (const posting of preview.postings) {
    const tr = document.createElement("tr");
    const acct = document.createElement("td");
    acct.textContent = posting.account;
    const d = document.createElement("td");
    d.className = "num";
    d.textContent = posting.side === "debit" ? formatCents(posting.amountMinor) : "";
    const c = document.createElement("td");
    c.className = "num";
    c.textContent = posting.side === "credit" ? formatCents(posting.amountMinor) : "";
    tr.append(acct, d, c);
    tbody.append(tr);
  }
  const tfoot = document.createElement("tfoot");
  const fr = document.createElement("tr");
  fr.innerHTML = `<td>Total</td><td class="num">${formatCents(preview.debits)}</td><td class="num">${formatCents(preview.credits)}</td>`;
  tfoot.append(fr);
  table.append(tbody, tfoot);
  wrap.append(table, flag(preview.debits, preview.credits));
  return wrap;
}

function flag(debits, credits) {
  const balanced = debits === credits && debits > 0;
  const empty = debits === 0 && credits === 0;
  const el = document.createElement("div");
  el.className = `balance-flag ${balanced ? "ok" : "bad"}`;
  if (empty) {
    el.className = "balance-flag bad";
    el.textContent = "Incomplete preview";
  } else if (balanced) {
    el.textContent = "Balanced · debits = credits";
  } else {
    const residual = debits - credits;
    el.textContent = `Unbalanced · residual ${formatCents(residual)} (debits ${formatCents(debits)} · credits ${formatCents(credits)})`;
  }
  return el;
}

function refresh(options = {}) {
  renderChips();
  const visible = visibleLines();
  renderCounts(visible);
  renderGrid(visible);
  renderPreview();
  if (!options.keepStatus) {
    /* leave status as-is unless the caller set it */
  }
  els.status.textContent = statusText;
}

function moveCursorTo(index, col = focusCol, extend = false) {
  if (index < 0 || index >= visibleIds.length) return;
  const id = visibleIds[index];
  if (!extend) selectedIds = new Set([id]);
  else {
    const anchor = visibleIds.indexOf([...selectedIds][0] ?? id);
    const from = Math.min(anchor, index);
    const to = Math.max(anchor, index);
    selectedIds = new Set(visibleIds.slice(from, to + 1));
    selectedIds.add(id);
  }
  cursor = index;
  focusCol = col;
}

function moveCursor(delta, extend = false) {
  moveCursorTo(cursor + delta, focusCol, extend);
}

function moveCol(delta) {
  const index = COLUMNS.indexOf(focusCol);
  const next = Math.min(COLUMNS.length - 1, Math.max(0, index + delta));
  focusCol = COLUMNS[next];
}

function editFieldForColumn(line) {
  if (focusCol === "category") {
    const recipe = line.recipeId ? getRecipe(line.recipeId) : undefined;
    if (recipe?.needsCounterAccount) return "counter";
    return "category";
  }
  return "recipe";
}

function optionLabels(field, line, query) {
  const q = query.trim().toLowerCase();
  /** @type {{ id: string, label: string, detail?: string }[]} */
  let items = [];
  if (field === "recipe") {
    items = RECIPES.map((recipe) => ({
      id: recipe.id,
      label: recipe.name,
      detail: recipe.description,
    }));
  } else if (field === "category") {
    const recipe = line.recipeId ? getRecipe(line.recipeId) : undefined;
    items = CATEGORIES.filter((category) => {
      if (recipe?.categoryType && category.type !== recipe.categoryType) return false;
      return true;
    }).map((category) => ({
      id: category.id,
      label: category.name,
      detail: `${category.type} · ${category.entity}`,
    }));
  } else {
    items = financialAccounts()
      .filter((account) => account !== line.account)
      .map((account) => ({ id: account, label: account }));
  }
  if (!q) return items;
  return items.filter(
    (item) =>
      item.label.toLowerCase().includes(q) ||
      item.id.toLowerCase().includes(q) ||
      (item.detail && item.detail.toLowerCase().includes(q)),
  );
}

function startEdit(field, initialQuery = "") {
  closeEdit();
  const id = visibleIds[cursor];
  const line = id ? byId(id) : undefined;
  if (!line || line.state === "voided" || line.state === "reconciled") {
    setStatus(line?.state === "reconciled" ? "Reopen the line (u) before editing." : "Nothing to edit.");
    return;
  }
  if (field === "category" && !line.recipeId) {
    startEdit("recipe", initialQuery);
    return;
  }
  const options = optionLabels(field, line, initialQuery);
  const current =
    field === "recipe"
      ? line.recipeId
      : field === "category"
        ? line.categoryId
        : line.counterAccount;
  const active = Math.max(0, options.findIndex((item) => item.id === current));
  editing = { lineId: line.id, field, query: initialQuery, active, options: options.map((o) => o.id) };
  renderCombo(line, options);
  const input = els.comboRoot.querySelector(".combo-input");
  if (input) {
    input.value = initialQuery;
    input.focus();
    input.select();
  }
}

function renderCombo(line, options) {
  const cell = els.body.querySelector(`tr[data-id="${CSS.escape(line.id)}"] td.focused`) ||
    els.body.querySelector(`tr[data-id="${CSS.escape(line.id)}"]`);
  const rect = (cell || els.grid).getBoundingClientRect();
  els.comboRoot.replaceChildren();
  const popup = document.createElement("div");
  popup.className = "combo-popup";
  popup.style.left = `${Math.round(rect.left)}px`;
  popup.style.top = `${Math.round(rect.bottom + 4)}px`;

  const input = document.createElement("input");
  input.className = "combo-input";
  input.type = "text";
  input.role = "combobox";
  input.setAttribute("aria-autocomplete", "list");
  input.setAttribute("aria-expanded", "true");
  input.setAttribute("aria-controls", "combo-list");
  input.setAttribute("spellcheck", "false");
  input.placeholder =
    editing.field === "recipe"
      ? "Recipe"
      : editing.field === "counter"
        ? "Other account"
        : "Leaf category";
  input.value = editing.query;
  input.addEventListener("input", () => {
    editing.query = input.value;
    const next = optionLabels(editing.field, line, editing.query);
    editing.options = next.map((o) => o.id);
    editing.active = 0;
    renderCombo(line, next);
    const again = els.comboRoot.querySelector(".combo-input");
    if (again) {
      again.focus();
      const pos = input.value.length;
      again.setSelectionRange(pos, pos);
    }
  });
  input.addEventListener("keydown", onComboKey);

  const list = document.createElement("ul");
  list.id = "combo-list";
  list.role = "listbox";
  if (options.length === 0) {
    const empty = document.createElement("div");
    empty.className = "combo-empty";
    empty.textContent = "No matches";
    popup.append(input, empty);
  } else {
    options.forEach((item, index) => {
      const li = document.createElement("li");
      li.id = `combo-opt-${index}`;
      li.role = "option";
      li.setAttribute("aria-selected", index === editing.active ? "true" : "false");
      li.textContent = item.detail ? `${item.label} — ${item.detail}` : item.label;
      li.addEventListener("mousedown", (event) => {
        event.preventDefault();
        editing.active = index;
        commitEdit();
      });
      list.append(li);
    });
    input.setAttribute("aria-activedescendant", `combo-opt-${editing.active}`);
    popup.append(input, list);
  }
  els.comboRoot.append(popup);
  const selected = list.querySelector('li[aria-selected="true"]');
  if (selected) selected.scrollIntoView({ block: "nearest" });
}

function closeEdit() {
  editing = null;
  els.comboRoot.replaceChildren();
}

function commitEdit() {
  if (!editing) return;
  const line = byId(editing.lineId);
  if (!line || editing.options.length === 0) {
    closeEdit();
    refresh();
    return;
  }
  const chosen = editing.options[Math.min(editing.active, editing.options.length - 1)];
  if (editing.field === "recipe") {
    line.recipeId = chosen;
    const recipe = getRecipe(chosen);
    if (!recipe?.needsCategory) line.categoryId = null;
    if (!recipe?.needsCounterAccount) line.counterAccount = null;
    if (!line.categoryId && recipe?.needsCategory) {
      const suggested = suggestCategoryFromHint(line.monarchCategory);
      if (suggested && (!recipe.categoryType || suggested.type === recipe.categoryType)) {
        line.categoryId = suggested.id;
      }
    }
    line.state = nextReviewState(line.state, "choose", { hasRecipe: true });
    setStatus(`Recipe set to ${recipe?.name || chosen}.`);
    const needsMore = recipe?.needsCategory && !line.categoryId
      ? "category"
      : recipe?.needsCounterAccount && !line.counterAccount
        ? "counter"
        : null;
    closeEdit();
    refresh();
    if (needsMore) {
      focusCol = "category";
      startEdit(needsMore);
      return;
    }
    return;
  }
  if (editing.field === "category") {
    line.categoryId = chosen;
    line.state = nextReviewState(line.state, "choose", { hasRecipe: Boolean(line.recipeId) });
    setStatus(`Category set to ${getCategory(chosen)?.name || chosen}.`);
  } else {
    line.counterAccount = chosen;
    line.state = nextReviewState(line.state, "choose", { hasRecipe: Boolean(line.recipeId) });
    setStatus(`Counter account set to ${chosen}.`);
  }
  closeEdit();
  refresh();
}

function onComboKey(event) {
  if (!editing) return;
  if (event.key === "ArrowDown") {
    event.preventDefault();
    event.stopPropagation();
    editing.active = Math.min(editing.options.length - 1, editing.active + 1);
    const line = byId(editing.lineId);
    renderCombo(line, optionLabels(editing.field, line, editing.query));
    els.comboRoot.querySelector(".combo-input")?.focus();
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    event.stopPropagation();
    editing.active = Math.max(0, editing.active - 1);
    const line = byId(editing.lineId);
    renderCombo(line, optionLabels(editing.field, line, editing.query));
    els.comboRoot.querySelector(".combo-input")?.focus();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    commitEdit();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    closeEdit();
    refresh();
    setStatus("Edit cancelled.");
    return;
  }
  if (event.key === "Tab") {
    event.preventDefault();
    event.stopPropagation();
    commitEdit();
    return;
  }
}

function acceptSelected() {
  const selected = visibleIds.map(byId).filter((line) => line && selectedIds.has(line.id));
  if (selected.length === 0) return;
  let posted = 0;
  let skipped = 0;
  for (const line of selected) {
    if (line.state === "voided" || line.state === "reconciled") {
      skipped += 1;
      continue;
    }
    if (!line.recipeId) {
      skipped += 1;
      continue;
    }
    const preview = linePreview(line);
    const ok = preview.balanced && preview.postings.length > 0;
    const next = nextReviewState(line.state, "accept", {
      balanced: ok,
      hasRecipe: Boolean(line.recipeId),
    });
    if (next === "reconciled") {
      line.state = next;
      posted += 1;
    } else skipped += 1;
  }
  if (posted && !skipped) setStatus(`Posted ${posted} line${posted === 1 ? "" : "s"}.`);
  else if (posted) setStatus(`Posted ${posted}, skipped ${skipped} (need a balanced recipe preview).`);
  else setStatus("Nothing posted. Choose a recipe so the preview balances, then press a.");
  refresh({ keepStatus: true });
}

function voidSelected() {
  const selected = visibleIds.map(byId).filter((line) => line && selectedIds.has(line.id));
  let count = 0;
  for (const line of selected) {
    if (line.state !== "voided") {
      line.state = nextReviewState(line.state, "void");
      count += 1;
    }
  }
  setStatus(count ? `Voided ${count} line${count === 1 ? "" : "s"}.` : "Nothing to void.");
  refresh({ keepStatus: true });
}

function reopenSelected() {
  const selected = visibleIds.map(byId).filter((line) => line && selectedIds.has(line.id));
  let count = 0;
  for (const line of selected) {
    if (line.state === "reconciled" || line.state === "voided") {
      line.state = nextReviewState(line.state, "reopen", { hasRecipe: Boolean(line.recipeId) });
      count += 1;
    } else if (line.state === "interpreted") {
      line.recipeId = null;
      line.categoryId = null;
      line.counterAccount = null;
      line.state = nextReviewState(line.state, "clear");
      count += 1;
    }
  }
  setStatus(count ? `Reopened ${count} line${count === 1 ? "" : "s"}.` : "Nothing to reopen.");
  refresh({ keepStatus: true });
}

function toggleHelp(open = !helpOpen) {
  helpOpen = open;
  els.help.hidden = !helpOpen;
  if (helpOpen) els.helpClose.focus();
  else refresh();
}

function onGridKey(event) {
  if (helpOpen) {
    if (event.key === "Escape" || event.key === "?") {
      event.preventDefault();
      toggleHelp(false);
    }
    return;
  }
  if (editing) return;
  const inFilter = document.activeElement === els.filter;
  if (inFilter) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (els.filter.value) {
        els.filter.value = "";
        filterText = "";
        refresh();
      } else {
        els.filter.blur();
        refresh();
      }
    }
    return;
  }

  const extend = event.shiftKey;
  if (event.key === "/" && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault();
    els.filter.focus();
    els.filter.select();
    return;
  }
  if (event.key === "?" ) {
    event.preventDefault();
    toggleHelp(true);
    return;
  }
  if (event.key === "j" || event.key === "ArrowDown") {
    event.preventDefault();
    moveCursor(1, extend);
    refresh();
    return;
  }
  if (event.key === "k" || event.key === "ArrowUp") {
    event.preventDefault();
    moveCursor(-1, extend);
    refresh();
    return;
  }
  if (event.key === "h" || event.key === "ArrowLeft") {
    event.preventDefault();
    moveCol(-1);
    refresh();
    return;
  }
  if (event.key === "l" || event.key === "ArrowRight") {
    event.preventDefault();
    moveCol(1);
    refresh();
    return;
  }
  if (event.key === "Home") {
    event.preventDefault();
    moveCursorTo(0, focusCol, extend);
    refresh();
    return;
  }
  if (event.key === "End") {
    event.preventDefault();
    moveCursorTo(visibleIds.length - 1, focusCol, extend);
    refresh();
    return;
  }
  if (event.key === " " && visibleIds[cursor]) {
    event.preventDefault();
    const id = visibleIds[cursor];
    if (selectedIds.has(id) && selectedIds.size > 1) selectedIds.delete(id);
    else selectedIds.add(id);
    refresh();
    return;
  }
  if (event.key === "Enter" || event.key === "e") {
    event.preventDefault();
    const line = byId(visibleIds[cursor]);
    if (!line) return;
    startEdit(editFieldForColumn(line));
    return;
  }
  if (event.key === "Escape") {
    if (selectedIds.size > 1 && visibleIds[cursor]) {
      selectedIds = new Set([visibleIds[cursor]]);
      setStatus("Selection cleared.");
      refresh({ keepStatus: true });
    }
    return;
  }
  if (event.key === "a" && !event.metaKey && !event.ctrlKey) {
    event.preventDefault();
    acceptSelected();
    return;
  }
  if (event.key === "x" && !event.metaKey && !event.ctrlKey) {
    event.preventDefault();
    voidSelected();
    return;
  }
  if (event.key === "u" && !event.metaKey && !event.ctrlKey) {
    event.preventDefault();
    reopenSelected();
    return;
  }

  if (
    event.key.length === 1 &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.altKey &&
    (focusCol === "recipe" || focusCol === "category") &&
    /[\p{L}\p{N} ]/u.test(event.key)
  ) {
    const line = byId(visibleIds[cursor]);
    if (line) {
      event.preventDefault();
      startEdit(editFieldForColumn(line), event.key);
    }
  }
}

els.filter.addEventListener("input", () => {
  filterText = els.filter.value;
  cursor = 0;
  selectedIds = new Set();
  refresh({ keepStatus: true });
});
els.helpClose.addEventListener("click", () => toggleHelp(false));
els.help.addEventListener("click", (event) => {
  if (event.target === els.help) toggleHelp(false);
});
document.addEventListener("keydown", onGridKey);

async function main() {
  const response = await fetch("/api/rows", { cache: "no-store" });
  if (!response.ok) {
    setStatus(`Failed to load rows (${response.status}).`);
    return;
  }
  const payload = await response.json();
  source = payload.source;
  observationCount = payload.observationCount;
  expectedCount = payload.expectedCount;
  lines = payload.lines;
  if (visibleLines()[0]) selectedIds = new Set([visibleLines()[0].id]);
  renderMeta();
  setStatus(`Loaded ${lines.length} lines from ${source === "YA_MONARCH_CSV" ? "YA_MONARCH_CSV" : "the synthetic fixture"}. Press ? for keys.`);
  refresh({ keepStatus: true });
}

main().catch(() => {
  setStatus("Failed to load rows.");
});
