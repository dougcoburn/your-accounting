import { parseCsv } from "./csv.ts";
import { FeedError } from "./errors.ts";
import { parseSignedMinor } from "./money.ts";
import type { Observation } from "./types.ts";

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

function normalizeDate(raw: string): string | null {
  const text = raw.trim();
  const iso = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text);
  const us = /^(\d{1,2})\/(\d{1,2})\/(\d{4})$/.exec(text);
  let year: number;
  let month: number;
  let day: number;
  if (iso) {
    year = Number(iso[1]);
    month = Number(iso[2]);
    day = Number(iso[3]);
  } else if (us) {
    month = Number(us[1]);
    day = Number(us[2]);
    year = Number(us[3]);
  } else {
    return null;
  }
  const date = new Date(Date.UTC(year, month - 1, day));
  if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) {
    return null;
  }
  return `${String(year).padStart(4, "0")}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

function tagsOf(raw: string): string[] {
  return raw.split(",").map((tag) => tag.trim()).filter((tag) => tag !== "");
}

export function readObservations(csv: string, postedDates: Record<string, string> | null): Observation[] {
  let table;
  try {
    table = parseCsv(csv);
  } catch {
    throw new FeedError("The feed could not be parsed.");
  }
  if (table.headers.length !== MONARCH_COLUMNS.length || MONARCH_COLUMNS.some((column, index) => table.headers[index] !== column)) {
    throw new FeedError("The feed does not have the 14 Monarch columns.");
  }
  const seen = new Set<string>();
  const observations: Observation[] = [];
  table.rows.forEach((row, index) => {
    const record = index + 1;
    if (row.length !== MONARCH_COLUMNS.length) {
      throw new FeedError(`Record ${record} does not have the expected columns.`);
    }
    const occurredOn = normalizeDate(row[0]);
    if (!occurredOn) {
      throw new FeedError(`Record ${record} has a missing or unrecognized date.`);
    }
    let amountMinor: number;
    try {
      amountMinor = parseSignedMinor(row[6]);
    } catch {
      throw new FeedError(`Record ${record} has an amount that is not a decimal.`);
    }
    const id = row[13].trim();
    if (id === "") {
      throw new FeedError(`Record ${record} has an empty Id.`);
    }
    if (seen.has(id)) {
      throw new FeedError(`Record ${record} repeats an earlier Id.`);
    }
    seen.add(id);
    const postedRaw = postedDates ? postedDates[id] : undefined;
    let postedOn: string | null = null;
    if (postedRaw !== undefined) {
      postedOn = normalizeDate(postedRaw);
      if (!postedOn) {
        throw new FeedError(`Record ${record} has an unrecognized posted date in the fixture.`);
      }
    }
    observations.push({
      id,
      occurredOn,
      postedOn,
      merchant: row[1].trim(),
      category: row[2].trim(),
      accountName: row[3].trim(),
      originalStatement: row[4],
      notes: row[5],
      amountMinor,
      tags: tagsOf(row[7]),
      owner: row[8].trim(),
      reviewed: row[9].trim(),
      reviewedAt: row[10].trim(),
      reviewedBy: row[11].trim(),
      pending: row[12].trim(),
    });
  });
  if (observations.length === 0) {
    throw new FeedError("The feed has no rows.");
  }
  return observations;
}
