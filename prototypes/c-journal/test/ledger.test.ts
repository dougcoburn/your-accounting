import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { MONARCH_COLUMNS, readObservations } from "../src/adapter.ts";
import { parseBooks } from "../src/chart.ts";
import { parseCsv } from "../src/csv.ts";
import { CorrectionError } from "../src/errors.ts";
import { applyDemonstration, commitCorrection, demonstrationState, previewCorrection } from "../src/ledger.ts";
import { parseSignedMinor } from "../src/money.ts";
import { buildLedger } from "../src/post.ts";
import {
  activeEventFor,
  categoryAccountId,
  equations,
  eventMagnitude,
  isActive,
  lineageFor,
  naturalBalance,
  supersedeProblems,
  trialTotals,
} from "../src/query.ts";
import { reconcileAccount } from "../src/reconcile.ts";
import type { Ledger } from "../src/types.ts";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function loadFixture(): Ledger {
  const csv = readFileSync(path.join(root, "fixtures", "synthetic-monarch.csv"), "utf8");
  const books = parseBooks(JSON.parse(readFileSync(path.join(root, "fixtures", "synthetic-books.json"), "utf8")));
  return buildLedger(csv, books, "fixture");
}

function account(ledger: Ledger, name: string) {
  const found = ledger.accounts.find((item) => item.name === name);
  assert.ok(found, name);
  return found;
}

test("fixture is synthetic Monarch-shaped data", () => {
  const csv = readFileSync(path.join(root, "fixtures", "synthetic-monarch.csv"), "utf8");
  const booksText = readFileSync(path.join(root, "fixtures", "synthetic-books.json"), "utf8");
  assert.ok(csv.startsWith("# synthetic"));
  assert.ok(booksText.includes("# synthetic"));
  assert.equal(csv.includes("ya-private-data"), false);
  const table = parseCsv(csv);
  assert.deepEqual(table.headers, [...MONARCH_COLUMNS]);
  assert.equal(table.rows.length, 15);
  const observations = readObservations(csv, null);
  assert.equal(new Set(observations.map((observation) => observation.id)).size, 15);
  const quoted = readObservations(
    "# synthetic\n" +
      `${MONARCH_COLUMNS.join(",")}\n` +
      "2025-10-01,\"Northwind, Market\",Groceries,Checking •1111,,\"line one\nline two\",-5.00,,Casey Morgan,true,2025-10-02,Casey Morgan,false,t-9\n",
    null,
  );
  assert.equal(quoted[0].merchant, "Northwind, Market");
  assert.equal(quoted[0].notes, "line one\nline two");
  assert.equal(quoted[0].amountMinor, -500);
  assert.equal(parseSignedMinor("-$1,240.50"), -124050);
});

test("every entry balances and the equation closes from postings", () => {
  const ledger = loadFixture();
  assert.equal(ledger.events.length, 18);
  assert.equal(ledger.unposted.length, 1);
  assert.equal(ledger.unposted[0].observationId, "syn-1007");
  const totals = trialTotals(ledger.events);
  assert.equal(totals.debit, totals.credit);
  for (const equation of equations(ledger.events, ledger.accounts, ledger.entities.map((entity) => entity.id))) {
    assert.equal(equation.discrepancy, 0, equation.entityId);
  }
  assert.equal(naturalBalance(ledger.events, account(ledger, "Household Visa •4410"), "2025-10-31"), 208725);
  assert.equal(naturalBalance(ledger.events, account(ledger, "Household Visa •4410"), null), 210324);
  assert.equal(naturalBalance(ledger.events, account(ledger, "Household Checking •4821"), "2025-10-31"), 314112);
  assert.equal(naturalBalance(ledger.events, account(ledger, "Personal Checking •1188"), null), 451055);
  assert.equal(naturalBalance(ledger.events, account(ledger, "Personal HSA •2207"), null), 5000);
  assert.equal(naturalBalance(ledger.events, account(ledger, "Dragonfly Checking •7730"), null), 77100);
  const groceries = ledger.accounts.find((item) => item.name === "Groceries");
  assert.ok(groceries);
  assert.equal(naturalBalance(ledger.events, groceries, null), 9960);
  const due = ledger.accounts.find((item) => item.name === "Due from Northline Studio");
  assert.ok(due);
  assert.equal(naturalBalance(ledger.events, due, null), 6000);
  const observation = ledger.observations.find((item) => item.id === "syn-1005");
  assert.equal(observation?.amountMinor, -95000);
});

test("occurred-date reconciliation explains both in-flight directions", () => {
  const ledger = loadFixture();
  const visa = account(ledger, "Household Visa •4410");
  const sheet = reconcileAccount(ledger, visa.id, "2025-10-31");
  assert.ok(sheet);
  assert.equal(sheet.occurredMinor, 208725);
  assert.deepEqual(sheet.bookNotStatement.map((line) => line.observationId), ["syn-1012"]);
  assert.deepEqual(sheet.statementNotBook.map((line) => line.observationId), ["syn-1014"]);
  assert.equal(sheet.unknownPosted.length, 0);
  assert.equal(sheet.explainedMinor, 207544);
  assert.equal(sheet.bookNotStatement[0].naturalMinor, 2780);
  assert.equal(sheet.statementNotBook[0].naturalMinor, 1599);
  const early = reconcileAccount(ledger, visa.id, "2025-10-01");
  assert.equal(early?.occurredMinor, 124000);
  assert.equal(early?.bookNotStatement.length, 0);
  assert.equal(early?.statementNotBook.length, 0);
  assert.equal(early?.explainedMinor, 124000);
  const later = reconcileAccount(ledger, visa.id, "2025-11-02");
  assert.equal(later?.occurredMinor, 210324);
  assert.equal(later?.explainedMinor, 210324);
  const checking = reconcileAccount(ledger, account(ledger, "Household Checking •4821").id, "2025-10-31");
  assert.equal(checking?.explainedMinor, 314112);
  assert.equal(checking?.bookNotStatement.length, 0);
  const hsa = reconcileAccount(ledger, account(ledger, "Personal HSA •2207").id, "2025-10-31");
  assert.equal(hsa?.occurredMinor, 5000);
  assert.equal(hsa?.unposted.length, 1);
});

test("correction of a correction hides the pair and keeps the full sum", () => {
  const ledger = loadFixture();
  const before = JSON.stringify(ledger.events);
  const original = activeEventFor(ledger.events, "syn-1005");
  assert.ok(original);
  previewCorrection(ledger, {
    eventId: original.id,
    amountMinor: 9000,
    categoryAccountId: "cat_df_office",
    reason: "preview only",
  });
  assert.equal(JSON.stringify(ledger.events), before);
  assert.equal(demonstrationState(ledger), "ready");
  assert.equal(applyDemonstration(ledger), 2);
  assert.equal(demonstrationState(ledger), "done");
  assert.equal(applyDemonstration(ledger), 0);

  const lineage = lineageFor(ledger.events, "syn-1005");
  assert.equal(lineage.length, 5);
  assert.deepEqual(lineage.map((event) => event.reversesId), [null, lineage[0].id, null, lineage[2].id, null]);
  assert.deepEqual(lineage.map((event) => event.supersededBy), [lineage[1].id, null, lineage[3].id, null, null]);
  assert.deepEqual(lineage.map((event) => eventMagnitude(event)), [95000, 95000, 9000, 9000, 9500]);
  assert.equal(categoryAccountId(lineage[4], ledger.accounts), "cat_df_software");
  assert.equal(lineage.filter(isActive).length, 1);
  assert.equal(isActive(lineage[4]), true);
  assert.deepEqual(supersedeProblems(ledger.events), []);
  assert.equal(ledger.observations.find((item) => item.id === "syn-1005")?.amountMinor, -95000);

  const totals = trialTotals(ledger.events);
  assert.equal(totals.debit, totals.credit);
  for (const equation of equations(ledger.events, ledger.accounts, ledger.entities.map((entity) => entity.id))) {
    assert.equal(equation.discrepancy, 0);
  }
  const visa = account(ledger, "Household Visa •4410");
  assert.equal(naturalBalance(ledger.events, visa, null), 124824);
  assert.equal(naturalBalance(ledger.events, visa, "2025-10-31"), 123225);
  const sheet = reconcileAccount(ledger, visa.id, "2025-10-31");
  assert.equal(sheet?.explainedMinor, 122044);
  assert.equal(sheet?.bookNotStatement[0].amountMinor, 2780);
  const office = ledger.accounts.find((item) => item.id === "cat_df_office");
  const software = ledger.accounts.find((item) => item.id === "cat_df_software");
  assert.ok(office && software);
  assert.equal(naturalBalance(ledger.events, office, null), 0);
  assert.equal(naturalBalance(ledger.events, software, null), 12400);
  assert.equal(naturalBalance(ledger.events, account(ledger, "Due from Dragonfly"), null), 9500);
});

test("a rejected correction leaves the journal unchanged", () => {
  const ledger = loadFixture();
  const before = JSON.stringify(ledger.events);
  const claim = activeEventFor(ledger.events, "syn-1002");
  const repayment = activeEventFor(ledger.events, "syn-1011");
  assert.ok(claim && repayment);
  assert.throws(
    () => commitCorrection(ledger, { eventId: claim.id, amountMinor: 3000, categoryAccountId: null, reason: "shrink the claim" }),
    CorrectionError,
  );
  assert.throws(
    () => commitCorrection(ledger, { eventId: repayment.id, amountMinor: 15000, categoryAccountId: null, reason: "too much" }),
    CorrectionError,
  );
  assert.throws(
    () => commitCorrection(ledger, { eventId: claim.id, amountMinor: 10000, categoryAccountId: null, reason: "same" }),
    CorrectionError,
  );
  assert.equal(JSON.stringify(ledger.events), before);
  assert.equal(ledger.observations.find((item) => item.id === "syn-1002")?.amountMinor, -10000);
});

test("a parent category is not posted and an inferred feed still balances", () => {
  const books = parseBooks(JSON.parse(readFileSync(path.join(root, "fixtures", "synthetic-books.json"), "utf8")));
  const header = MONARCH_COLUMNS.join(",");
  const parent = buildLedger(
    `# synthetic\n${header}\n2025-10-03,Northwind Market,Food,Household Visa •4410,,,-12.00,,Casey Morgan,true,2025-10-04,Casey Morgan,false,syn-x\n`,
    books,
    "fixture",
  );
  assert.equal(parent.unposted.length, 1);
  assert.match(parent.unposted[0].reason, /leaf/);
  assert.equal(parent.events.every((event) => event.memo === "Opening balance"), true);

  const inferred = buildLedger(
    `# synthetic\n${header}\n2025-10-01,Northwind Market,Groceries,Checking •1111,,,-10.50,,Casey Morgan,true,2025-10-02,Casey Morgan,false,t-1\n`,
    null,
    "env",
  );
  const totals = trialTotals(inferred.events);
  assert.equal(totals.debit, totals.credit);
  assert.equal(equations(inferred.events, inferred.accounts, inferred.entities.map((entity) => entity.id))[0].discrepancy, 0);
  assert.equal(inferred.events.length, 1);
  assert.equal(inferred.defaultCutoff, "2025-10-31");
});

test("server binds loopback and does not log rows", async () => {
  const env = { ...process.env };
  delete env.YA_MONARCH_CSV;
  const child = spawn(process.execPath, ["--experimental-strip-types", path.join(root, "server.ts")], {
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk: Buffer) => {
    stdout += chunk.toString("utf8");
  });
  child.stderr.on("data", (chunk: Buffer) => {
    stderr += chunk.toString("utf8");
  });
  try {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("server did not start")), 8000);
      child.stdout.on("data", () => {
        if (stdout.includes("listening")) {
          clearTimeout(timer);
          resolve();
        }
      });
      child.on("exit", (code) => {
        clearTimeout(timer);
        reject(new Error(`server exited ${code}`));
      });
    });
    const response = await fetch("http://127.0.0.1:5173/api/source");
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("cache-control"), "no-store");
    assert.equal(response.headers.get("set-cookie"), null);
    assert.match(response.headers.get("content-security-policy") ?? "", /default-src 'none'/);
    const body = await response.json() as { source: string; csv: string; books: { marker?: string } };
    assert.equal(body.source, "fixture");
    assert.equal(body.csv.includes("# synthetic"), true);
    assert.equal(body.books.marker?.includes("# synthetic"), true);
    const script = await fetch("http://127.0.0.1:5173/src/app.ts");
    assert.equal(script.headers.get("content-type"), "application/javascript; charset=utf-8");
    const page = await fetch("http://127.0.0.1:5173/");
    assert.equal(page.status, 200);
    const leaked = /Northwind|Harbor|950\.00|syn-1005/.test(stdout + stderr);
    assert.equal(leaked, false);
  } finally {
    child.kill("SIGTERM");
  }
});

test("client and server do not use browser storage or remote hosts", () => {
  const files = [
    ...readdirSync(path.join(root, "src")).map((name) => path.join(root, "src", name)),
    path.join(root, "server.ts"),
    path.join(root, "index.html"),
    path.join(root, "styles.css"),
  ];
  const banned = ["localStorage", "sessionStorage", "indexedDB", "document.cookie", "XMLHttpRequest", "eval("];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    for (const word of banned) {
      assert.equal(text.includes(word), false, `${path.basename(file)} ${word}`);
    }
  }
  const client = readdirSync(path.join(root, "src")).map((name) => readFileSync(path.join(root, "src", name), "utf8")).join("\n");
  assert.equal(/https?:\/\//.test(client), false);
  assert.equal(client.includes("console."), false);
});
