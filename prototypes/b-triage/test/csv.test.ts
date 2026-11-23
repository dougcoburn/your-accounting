import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { parseMonarchCsv } from "../src/csv.ts";
import { parseMinor } from "../src/money.ts";

const root = dirname(fileURLToPath(new URL(".", import.meta.url)));
const fixture = join(root, "data", "synthetic.csv");

describe("parseMinor", () => {
  it("parses signed decimal amounts without float drift", () => {
    assert.equal(parseMinor("10.20"), 1020);
    assert.equal(parseMinor("-0.07"), -7);
    assert.equal(parseMinor("1,200.00"), 120000);
    assert.equal(parseMinor("($12.50)"), -1250);
    assert.equal(parseMinor("10"), 1000);
  });
});

describe("parseMonarchCsv", () => {
  it("skips the # synthetic comment and reads 14-column rows", () => {
    const text = readFileSync(fixture, "utf8");
    const result = parseMonarchCsv(text);
    assert.equal(result.skippedComments, 1);
    assert.equal(result.issues.length, 0);
    assert.equal(result.observations.length, 37);
    const first = result.observations[0]!;
    assert.equal(first.id, "syn-001");
    assert.equal(first.date, "2025-10-01");
    assert.equal(first.reviewed, true);
    assert.equal(first.pending, false);
  });

  it("honors Reviewed and Pending booleans, including empties as false", () => {
    const text = [
      "# synthetic",
      "Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id",
      "2025-10-01,A,Cat,Acct *0001,STMT,,-1.00,e-x,Alex,false,,,true,t-1",
      "2025-10-01,B,Cat,Acct *0001,STMT,,-1.00,e-x,Alex,TRUE,2025-10-02,Alex,False,t-2",
    ].join("\n");
    const result = parseMonarchCsv(text);
    assert.equal(result.issues.length, 0);
    assert.equal(result.observations[0]?.reviewed, false);
    assert.equal(result.observations[0]?.pending, true);
    assert.equal(result.observations[1]?.reviewed, true);
    assert.equal(result.observations[1]?.pending, false);
  });

  it("keeps a quoted multiline Notes field as one row", () => {
    const text = [
      "# synthetic",
      "Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id",
      '2025-10-01,A,Cat,Acct *0001,STMT,"line 1',
      'line 2",-1.00,e-x,Alex,false,,,false,t-ml',
    ].join("\n");
    const result = parseMonarchCsv(text);
    assert.equal(result.issues.length, 0);
    assert.equal(result.observations.length, 1);
    assert.equal(result.observations[0]?.notes, "line 1\nline 2");
    assert.equal(result.observations[0]?.amountMinor, -100);
  });
});
