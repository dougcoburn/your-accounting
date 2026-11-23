import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { MONARCH_COLUMNS, parseCsv, parseMonarchCsv } from "../lib/csv.mjs";
import { toRegisterLines } from "../lib/adapter.mjs";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

describe("parseCsv", () => {
  it("skips a leading # synthetic comment and reads the header", () => {
    const rows = parseCsv("# synthetic\nA,B\n1,2\n");
    assert.deepEqual(rows, [
      ["A", "B"],
      ["1", "2"],
    ]);
  });

  it("keeps quoted commas and escaped quotes", () => {
    const rows = parseCsv('Name,Notes\n"Northwind, Inc.","He said ""hello"""\n');
    assert.deepEqual(rows[1], ["Northwind, Inc.", 'He said "hello"']);
  });

  it("keeps newlines inside quoted fields", () => {
    const rows = parseCsv('Name,Notes\n"Relecloud","Line 1\nLine 2"\n');
    assert.deepEqual(rows[1], ["Relecloud", "Line 1\nLine 2"]);
  });

  it("rejects an unterminated quote", () => {
    assert.throws(() => parseCsv('A,B\n"oops,1\n'), /Unterminated/);
  });
});

describe("synthetic October fixture", () => {
  const text = fs.readFileSync(path.join(root, "data/synthetic-october.csv"), "utf8");

  it("starts with # synthetic and the exact 14 Monarch columns", () => {
    assert.ok(text.startsWith("# synthetic\n"));
    const records = parseMonarchCsv(text);
    assert.deepEqual(
      parseCsv(text)[0],
      [...MONARCH_COLUMNS],
    );
    assert.ok(records.length >= 30 && records.length <= 60);
  });

  it("parses the multiline quoted note and comma-containing notes", () => {
    const records = parseMonarchCsv(text);
    const hosting = records.find((row) => row.Id === "syn-oct26-031");
    assert.equal(hosting.Notes, "Called support.\nTicket 4421, follow up next week.");
    const lunch = records.find((row) => row.Id === "syn-oct26-021");
    assert.equal(lunch.Notes, "Lunch with a client, reimbursable");
  });

  it("loads observations plus synthetic expected movements", () => {
    const { lines, observationCount, expectedCount } = toRegisterLines(
      parseMonarchCsv(text),
      { includeSyntheticExpected: true },
    );
    assert.equal(observationCount, 50);
    assert.ok(expectedCount >= 3);
    assert.equal(lines.length, observationCount + expectedCount);
    assert.ok(lines.every((line) => line.state === "unreviewed"));
    const origins = new Set(lines.map((line) => line.origin));
    assert.deepEqual([...origins].sort(), ["expected", "observed"]);
    const hsa = lines.filter((line) => line.id.startsWith("expected-hsa-"));
    assert.equal(hsa.length, 2);
    assert.ok(hsa.every((line) => line.amountMinor === 20000 && line.origin === "expected"));
  });
});
