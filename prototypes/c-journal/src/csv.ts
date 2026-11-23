export interface CsvTable {
  headers: string[];
  rows: string[][];
}

export function parseCsv(text: string): CsvTable {
  let source = text;
  if (source.charCodeAt(0) === 0xfeff) {
    source = source.slice(1);
  }
  if (source.startsWith("#")) {
    const newline = source.indexOf("\n");
    source = newline === -1 ? "" : source.slice(newline + 1);
  }
  const records: string[][] = [];
  let field = "";
  let row: string[] = [];
  let inQuotes = false;
  for (let i = 0; i < source.length; i += 1) {
    const char = source[i];
    if (inQuotes) {
      if (char === '"') {
        if (source[i + 1] === '"') {
          field += '"';
          i += 1;
        } else {
          inQuotes = false;
        }
      } else {
        field += char;
      }
      continue;
    }
    if (char === '"') {
      inQuotes = true;
      continue;
    }
    if (char === ",") {
      row.push(field);
      field = "";
      continue;
    }
    if (char === "\n" || char === "\r") {
      if (char === "\r" && source[i + 1] === "\n") {
        i += 1;
      }
      row.push(field);
      field = "";
      if (!(row.length === 1 && row[0] === "")) {
        records.push(row);
      }
      row = [];
      continue;
    }
    field += char;
  }
  if (inQuotes) {
    throw new Error("CSV has an unterminated quote");
  }
  if (field.length > 0 || row.length > 0) {
    row.push(field);
    if (!(row.length === 1 && row[0] === "")) {
      records.push(row);
    }
  }
  if (records.length === 0) {
    throw new Error("CSV has no header");
  }
  return {
    headers: records[0].map((header) => header.trim()),
    rows: records.slice(1),
  };
}
