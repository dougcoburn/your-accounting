import type { Observation } from "./types.ts";
import { parseMinor } from "./money.ts";

export const MONARCH_COLUMNS = [
  "Date",
  "Merchant",
  "Category",
  "Account",
  "Original Statement",
  "Notes",
  "Amount",
  "Tags",
  "Owner",
  "Reviewed",
  "Reviewed At",
  "Reviewed By",
  "Pending",
  "Id",
] as const;

export type ParseIssue = {
  line: number;
  /** Structural reason only — never includes field values. */
  reason: string;
};

export type ParseResult = {
  observations: Observation[];
  issues: ParseIssue[];
  skippedComments: number;
};

/**
 * RFC-style CSV parse that keeps quoted newlines and ignores `#` comment
 * lines (so a leading `# synthetic` marker is tolerated).
 */
export function parseCsvRecords(text: string): { rows: string[][]; issues: ParseIssue[]; skippedComments: number } {
  const rows: string[][] = [];
  const issues: ParseIssue[] = [];
  let skippedComments = 0;
  let field = "";
  let row: string[] = [];
  let inQuotes = false;
  let line = 1;
  let rowStartLine = 1;

  const pushRow = () => {
    const fields = [...row, field];
    const first = (fields[0] ?? "").trimStart();
    const onlyComment = !inQuotes && first.startsWith("#") && fields.every((f, i) => i === 0 || f.trim() === "");
    if (onlyComment) {
      skippedComments += 1;
    } else if (fields.length === 1 && fields[0].trim() === "") {
      // skip blank lines
    } else {
      rows.push(fields);
    }
    field = "";
    row = [];
    rowStartLine = line + 1;
  };

  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i]!;
    if (inQuotes) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i += 1;
        } else {
          inQuotes = false;
        }
      } else {
        if (ch === "\n") line += 1;
        field += ch;
      }
      continue;
    }
    if (ch === '"') {
      inQuotes = true;
      continue;
    }
    if (ch === ",") {
      row.push(field);
      field = "";
      continue;
    }
    if (ch === "\n") {
      if (field.endsWith("\r")) field = field.slice(0, -1);
      pushRow();
      line += 1;
      continue;
    }
    if (ch === "\r") continue;
    field += ch;
  }
  if (inQuotes) {
    issues.push({ line: rowStartLine, reason: "unterminated quoted field" });
  }
  if (field.length > 0 || row.length > 0) {
    pushRow();
  }
  return { rows, issues, skippedComments };
}

function parseBool(raw: string, fieldName: string, line: number, issues: ParseIssue[]): boolean {
  const v = raw.trim().toLowerCase();
  if (v === "" || v === "false" || v === "0" || v === "no") return false;
  if (v === "true" || v === "1" || v === "yes") return true;
  issues.push({ line, reason: `invalid boolean in ${fieldName}` });
  return false;
}

export function parseMonarchCsv(text: string): ParseResult {
  const { rows, issues, skippedComments } = parseCsvRecords(text);
  const observations: Observation[] = [];
  if (rows.length === 0) {
    issues.push({ line: 1, reason: "missing header row" });
    return { observations, issues, skippedComments };
  }

  const header = rows[0]!.map((h) => h.trim());
  const index = new Map(header.map((h, i) => [h, i]));
  const missing = MONARCH_COLUMNS.filter((c) => !index.has(c));
  if (missing.length > 0) {
    issues.push({ line: 1, reason: `missing columns: ${missing.join(", ")}` });
    return { observations, issues, skippedComments };
  }

  const seenIds = new Set<string>();
  for (let r = 1; r < rows.length; r += 1) {
    const row = rows[r]!;
    // Physical line numbers are approximate once quoted newlines appear;
    // report the 1-based record index plus header/comments as a stable locator.
    const line = r + 1 + skippedComments;
    const col = (name: (typeof MONARCH_COLUMNS)[number]) => (row[index.get(name)!] ?? "").trim();
    const id = col("Id");
    if (!id) {
      issues.push({ line, reason: "missing Id" });
      continue;
    }
    if (seenIds.has(id)) {
      issues.push({ line, reason: "duplicate Id" });
      continue;
    }
    let amountMinor: number;
    try {
      amountMinor = parseMinor(col("Amount"));
    } catch {
      issues.push({ line, reason: "invalid Amount" });
      continue;
    }
    const date = col("Date");
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) {
      issues.push({ line, reason: "invalid Date (expected YYYY-MM-DD)" });
      continue;
    }
    seenIds.add(id);
    observations.push({
      id,
      date,
      merchant: col("Merchant"),
      category: col("Category"),
      account: col("Account"),
      originalStatement: col("Original Statement"),
      notes: col("Notes"),
      amountMinor,
      tags: col("Tags"),
      owner: col("Owner"),
      reviewed: parseBool(col("Reviewed"), "Reviewed", line, issues),
      reviewedAt: col("Reviewed At"),
      reviewedBy: col("Reviewed By"),
      pending: parseBool(col("Pending"), "Pending", line, issues),
    });
  }

  return { observations, issues, skippedComments };
}
