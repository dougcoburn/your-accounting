import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { parseMonarchCsv } from "../src/csv.ts";
import {
  applyTreatment,
  booksFromObservations,
  entryIsBalanced,
  inboxLines,
  progress,
  skipToWorklist,
  voidLine,
} from "../src/ledger.ts";
import {
  NORTHWIND_STUB,
  findCardPaymentCounterpart,
  findExpectedHsa,
  findTransferCounterpart,
  paycheckSplitFor,
  suggestAll,
  suggestTreatment,
} from "../src/suggestions.ts";
import type { Observation } from "../src/types.ts";

const root = dirname(fileURLToPath(new URL(".", import.meta.url)));
const fixture = join(root, "data", "synthetic.csv");

function loadFixture(): Observation[] {
  return parseMonarchCsv(readFileSync(fixture, "utf8")).observations;
}

function byId(all: Observation[], id: string): Observation {
  const row = all.find((o) => o.id === id);
  assert.ok(row, `missing ${id}`);
  return row;
}

describe("card payment matching", () => {
  it("pairs a bank outflow with the matching card payment", () => {
    const all = loadFixture();
    const bank = byId(all, "syn-013");
    const card = byId(all, "syn-014");
    assert.equal(findCardPaymentCounterpart(bank, all)?.id, "syn-014");
    assert.equal(findCardPaymentCounterpart(card, all)?.id, "syn-013");
    const treatment = suggestTreatment(bank, all);
    assert.equal(treatment.recipeId, "card-payment");
    assert.equal(treatment.settlementGroup?.complete, true);
    assert.equal(treatment.settlementGroup?.residualMinor, 0);
    const ids = treatment.settlementGroup?.participants.map((p) => p.observationId).sort();
    assert.deepEqual(ids, ["syn-013", "syn-014"]);
  });

  it("does not pair different amounts", () => {
    const all = loadFixture();
    const bank1200 = byId(all, "syn-013");
    const card200 = byId(all, "syn-018");
    assert.notEqual(findCardPaymentCounterpart(bank1200, all)?.id, card200.id);
    // The $75 bank-only payment has no card counterpart.
    const orphan = byId(all, "syn-019");
    assert.equal(findCardPaymentCounterpart(orphan, all), undefined);
    assert.equal(suggestTreatment(orphan, all).settlementGroup?.complete, false);
  });

  it("does not pair outside the date window", () => {
    const all = loadFixture();
    const bank = byId(all, "syn-013");
    const farCard: Observation = {
      ...byId(all, "syn-014"),
      id: "far-card",
      date: "2025-11-15",
    };
    const withoutNear = all.filter((o) => o.id !== "syn-014").concat(farCard);
    assert.equal(findCardPaymentCounterpart(bank, withoutNear), undefined);
  });

  it("picks a deterministic counterpart when two candidates tie on amount", () => {
    const all = loadFixture();
    const bank = byId(all, "syn-013");
    const card = byId(all, "syn-014");
    const twin: Observation = { ...card, id: "syn-014b", merchant: "Payment - Thank You Twin" };
    const a = findCardPaymentCounterpart(bank, [...all, twin]);
    const b = findCardPaymentCounterpart(bank, [twin, ...all].reverse());
    assert.equal(a?.id, b?.id);
  });
});

describe("paycheck splits and Expected HSA", () => {
  it("splits a Northwind paycheck into gross, taxes, pre-tax, Roth, HSA, and net", () => {
    const all = loadFixture();
    const first = byId(all, "syn-020");
    const split = paycheckSplitFor(first);
    assert.equal(split.grossMinor, NORTHWIND_STUB.grossMinor);
    assert.equal(split.taxesMinor, NORTHWIND_STUB.taxesMinor);
    assert.equal(split.pretaxMinor, NORTHWIND_STUB.pretaxMinor);
    assert.equal(split.rothMinor, NORTHWIND_STUB.rothMinor);
    assert.equal(split.hsaMinor, NORTHWIND_STUB.hsaMinor);
    assert.equal(split.netMinor, NORTHWIND_STUB.netMinor);
    assert.equal(
      split.grossMinor - split.taxesMinor - split.pretaxMinor - split.rothMinor - split.hsaMinor,
      split.netMinor,
    );
    const group = suggestTreatment(first, all).settlementGroup;
    assert.ok(group);
    const roles = group.participants.map((p) => p.role);
    assert.deepEqual(roles, ["gross", "taxes", "pre-tax", "roth", "hsa", "net", "expected-hsa"]);
  });

  it("leaves Expected HSA unfilled when no HSA inflow exists", () => {
    const all = loadFixture();
    const first = byId(all, "syn-020");
    // Oct 3 paycheck should not pick up the Oct 18 HSA (that's for the later payday).
    const hsa = findExpectedHsa(first, all, NORTHWIND_STUB.hsaMinor);
    assert.equal(hsa, undefined);
    const slot = suggestTreatment(first, all).settlementGroup?.participants.find((p) => p.role === "expected-hsa");
    assert.equal(slot?.filled, false);
    assert.equal(slot?.origin, "expected");
  });

  it("fills Expected HSA when a matching HSA inflow sits in the window", () => {
    const all = loadFixture();
    const second = byId(all, "syn-021");
    const hsa = findExpectedHsa(second, all, NORTHWIND_STUB.hsaMinor);
    assert.equal(hsa?.id, "syn-022");
    const slot = suggestTreatment(second, all).settlementGroup?.participants.find((p) => p.role === "expected-hsa");
    assert.equal(slot?.filled, true);
    assert.equal(slot?.observationId, "syn-022");
  });
});

describe("transfer matching", () => {
  it("pairs opposite-sign own-account transfers of the same amount", () => {
    const all = loadFixture();
    const outLeg = byId(all, "syn-023");
    const inLeg = byId(all, "syn-024");
    assert.equal(findTransferCounterpart(outLeg, all)?.id, "syn-024");
    assert.equal(findTransferCounterpart(inLeg, all)?.id, "syn-023");
  });
});

describe("Monarch Reviewed default and honor toggle", () => {
  it("ignores Monarch Reviewed by default so every row starts unreviewed", () => {
    const all = loadFixture();
    assert.ok(all.some((o) => o.reviewed), "fixture should include reviewed rows");
    const books0 = booksFromObservations(all);
    const start = progress(books0);
    assert.equal(start.unreviewed, all.length);
    assert.equal(start.unreviewed, start.total);
    assert.equal(start.reconciled, 0);
    assert.equal(inboxLines(books0).length, all.length);
    const reviewed = books0.lines.find((l) => l.observation?.id === "syn-001");
    assert.equal(reviewed?.observation?.reviewed, true);
    assert.equal(reviewed?.state, "unreviewed");
  });

  it("honors Monarch Reviewed when the option is on", () => {
    const all = loadFixture();
    const honored = booksFromObservations(all, { honorMonarchReviewed: true });
    const p = progress(honored);
    assert.equal(p.reconciled, 7);
    assert.equal(p.unreviewed, 30);
    assert.equal(p.total, all.length);
    assert.equal(honored.lines.find((l) => l.observation?.id === "syn-001")?.state, "reconciled");
    assert.equal(honored.lines.find((l) => l.observation?.id === "syn-006")?.state, "unreviewed");
    assert.equal(inboxLines(honored).length, 30);
  });
});

describe("toy ledger apply", () => {
  it("applies a card payment in one click and posts a balanced entry for both legs", () => {
    const all = loadFixture();
    const books0 = booksFromObservations(all);
    const start = progress(books0);
    assert.equal(start.reconciled, 0);
    assert.equal(start.unreviewed, all.length);
    const treatment = suggestAll(all).get("syn-013")!;
    const result = applyTreatment(books0, "syn-013", treatment);
    assert.equal(result.ok, true);
    const books = result.books;
    const bank = books.lines.find((l) => l.observation?.id === "syn-013");
    const card = books.lines.find((l) => l.observation?.id === "syn-014");
    assert.equal(bank?.state, "reconciled");
    assert.equal(card?.state, "reconciled");
    const entry = books.entries[books.entries.length - 1]!;
    assert.equal(entryIsBalanced(entry), true);
    assert.equal(inboxLines(books).some((l) => l.observation?.id === "syn-014"), false);
    const next = progress(books);
    assert.equal(next.reconciled, start.reconciled + 2);
    assert.equal(next.reached, start.reached + 2);
  });

  it("opens an Expected HSA worklist slot when the paycheck HSA is unfilled", () => {
    const all = loadFixture();
    const books0 = booksFromObservations(all);
    const treatment = suggestAll(all).get("syn-020")!;
    const result = applyTreatment(books0, "syn-020", treatment);
    assert.equal(result.ok, true);
    const pay = result.books.lines.find((l) => l.observation?.id === "syn-020");
    assert.equal(pay?.state, "reconciled");
    const expected = result.books.lines.filter((l) => l.origin === "expected");
    assert.equal(expected.length, 1);
    assert.equal(expected[0]?.expected?.label, "Expected HSA");
    assert.equal(expected[0]?.state, "open");
    const entry = result.books.entries[result.books.entries.length - 1]!;
    assert.equal(entryIsBalanced(entry), true);
  });

  it("skip and void move a line out of the inbox and count toward progress", () => {
    const all = loadFixture();
    const books0 = booksFromObservations(all);
    const start = progress(books0);
    const skipped = skipToWorklist(books0, "syn-006");
    assert.equal(skipped.ok, true);
    const voided = voidLine(skipped.books, "syn-007");
    assert.equal(voided.ok, true);
    const p = progress(voided.books);
    assert.equal(p.open, 1);
    assert.equal(p.voided, 1);
    assert.equal(p.reached, start.reached + 2);
    assert.equal(
      inboxLines(voided.books).some((l) => l.observation?.id === "syn-006" || l.observation?.id === "syn-007"),
      false,
    );
  });
});
