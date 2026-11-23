/**
 * RFC 4180-style CSV helpers for the throwaway register prototype.
 * No runtime dependencies.
 */

/** @typedef {Record<string, string>} CsvRecord */

export const MONARCH_COLUMNS = Object.freeze([
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
]);

/**
 * Parse CSV text into a matrix of fields.
 * Skips a leading `# synthetic` (or any leading `# …`) comment line.
 * Quoted fields may contain commas, escaped quotes (`""`), and newlines.
 *
 * @param {string} text
 * @returns {string[][]}
 */
export function parseCsv(text) {
  if (typeof text !== "string") {
    throw new TypeError("parseCsv expects a string");
  }
  let source = text;
  if (source.charCodeAt(0) === 0xfeff) {
    source = source.slice(1);
  }
  source = source.replace(/\r\n/g, "\n").replace(/\r/g, "\n");

  const lines = source.split("\n");
  let skip = 0;
  while (skip < lines.length && lines[skip].trimStart().startsWith("#")) {
    skip += 1;
  }
  const body = lines.slice(skip).join("\n");
  return parseCsvBody(body);
}

/**
 * @param {string} source
 * @returns {string[][]}
 */
function parseCsvBody(source) {
  /** @type {string[][]} */
  const rows = [];
  /** @type {string[]} */
  let row = [];
  let field = "";
  let inQuotes = false;
  let i = 0;

  const pushField = () => {
    row.push(field);
    field = "";
  };
  const pushRow = () => {
    const empty = row.length === 1 && row[0] === "";
    if (!empty) rows.push(row);
    row = [];
  };

  while (i < source.length) {
    const char = source[i];
    if (inQuotes) {
      if (char === '"') {
        if (source[i + 1] === '"') {
          field += '"';
          i += 2;
          continue;
        }
        inQuotes = false;
        i += 1;
        continue;
      }
      field += char;
      i += 1;
      continue;
    }
    if (char === '"') {
      inQuotes = true;
      i += 1;
      continue;
    }
    if (char === ",") {
      pushField();
      i += 1;
      continue;
    }
    if (char === "\n") {
      pushField();
      pushRow();
      i += 1;
      continue;
    }
    field += char;
    i += 1;
  }

  if (inQuotes) {
    throw new SyntaxError("Unterminated quoted field in CSV");
  }
  if (field.length > 0 || row.length > 0) {
    pushField();
    pushRow();
  }
  return rows;
}

/**
 * Parse a Monarch-shaped CSV (exactly the 14 known columns, in order).
 *
 * @param {string} text
 * @returns {CsvRecord[]}
 */
export function parseMonarchCsv(text) {
  const rows = parseCsv(text);
  if (rows.length === 0) {
    throw new Error("CSV has no header row");
  }
  const header = rows[0].map((cell) => cell.trim());
  const mismatch =
    header.length !== MONARCH_COLUMNS.length ||
    MONARCH_COLUMNS.some((name, index) => name !== header[index]);
  if (mismatch) {
    throw new Error(`CSV header must be exactly: ${MONARCH_COLUMNS.join(",")}`);
  }

  /** @type {CsvRecord[]} */
  const records = [];
  for (let r = 1; r < rows.length; r += 1) {
    const cells = rows[r];
    const blank = cells.every((cell) => cell.trim() === "");
    if (blank) continue;
    if (cells.length !== header.length) {
      throw new Error(
        `Row ${r + 1} has ${cells.length} fields, expected ${header.length}`,
      );
    }
    /** @type {CsvRecord} */
    const record = {};
    for (let c = 0; c < header.length; c += 1) {
      record[header[c]] = cells[c];
    }
    records.push(record);
  }
  return records;
}
