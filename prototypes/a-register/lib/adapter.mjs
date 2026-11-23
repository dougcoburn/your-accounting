/**
 * Monarch CSV → in-memory register lines (observations + optional expected movements).
 */

import { parseAmountToMinor } from "./money.mjs";
import { inferAccountType } from "./recipes.mjs";

/**
 * @typedef {object} RegisterLine
 * @property {string} id
 * @property {string} occurredDate ISO date YYYY-MM-DD
 * @property {string} account
 * @property {string} accountLast4
 * @property {string} accountType
 * @property {string} merchant
 * @property {number} amountMinor signed integer cents (negative = outflow)
 * @property {"USD"} currency
 * @property {string} monarchCategory
 * @property {string} originalStatement
 * @property {string} notes
 * @property {string[]} tags
 * @property {string} owner
 * @property {boolean} pending
 * @property {boolean} monarchReviewed hint only; not review-state truth
 * @property {"observed" | "expected"} origin
 * @property {"unreviewed" | "interpreted" | "reconciled" | "voided"} state
 * @property {string | null} recipeId
 * @property {string | null} categoryId
 * @property {string | null} counterAccount
 * @property {string} [entityHint]
 */

/**
 * @param {string} account
 * @returns {string}
 */
export function accountLast4(account) {
  const match = String(account).match(/(\d{4})\s*$/);
  return match ? match[1] : "";
}

/**
 * @param {string} raw
 * @returns {string}
 */
export function normalizeDate(raw) {
  const text = String(raw).trim();
  const iso = text.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (iso) return text;
  const us = text.match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})$/);
  if (us) {
    const month = us[1].padStart(2, "0");
    const day = us[2].padStart(2, "0");
    return `${us[3]}-${month}-${day}`;
  }
  throw new SyntaxError(`unrecognized date: ${raw}`);
}

/**
 * @param {string} raw
 * @returns {boolean}
 */
export function parseBool(raw) {
  const text = String(raw).trim().toLowerCase();
  return text === "true" || text === "yes" || text === "1";
}

/**
 * @param {string} raw
 * @returns {string[]}
 */
export function parseTags(raw) {
  return String(raw)
    .split(/[;,]/)
    .map((tag) => tag.trim())
    .filter(Boolean);
}

/**
 * @param {string[]} tags
 * @returns {string}
 */
export function entityHintFromTags(tags) {
  for (const tag of tags) {
    const match = tag.match(/^e-(.+)$/i);
    if (match) {
      const slug = match[1].toLowerCase();
      if (slug === "doug") return "Doug";
      if (slug === "household") return "Household";
      if (slug === "dragonfly") return "Dragonfly";
      return match[1];
    }
  }
  return "";
}

/**
 * @param {import("./csv.mjs").CsvRecord} record
 * @returns {RegisterLine}
 */
export function observationFromRecord(record) {
  const id = String(record.Id || "").trim();
  if (!id) throw new Error("observation is missing Id");
  const tags = parseTags(record.Tags || "");
  const account = String(record.Account || "").trim();
  return {
    id,
    occurredDate: normalizeDate(record.Date || ""),
    account,
    accountLast4: accountLast4(account),
    accountType: inferAccountType(account),
    merchant: String(record.Merchant || "").trim(),
    amountMinor: parseAmountToMinor(record.Amount || ""),
    currency: "USD",
    monarchCategory: String(record.Category || "").trim(),
    originalStatement: String(record["Original Statement"] || "").trim(),
    notes: String(record.Notes || "").trim(),
    tags,
    owner: String(record.Owner || "").trim(),
    pending: parseBool(record.Pending || ""),
    monarchReviewed: parseBool(record.Reviewed || ""),
    origin: "observed",
    state: "unreviewed",
    recipeId: null,
    categoryId: null,
    counterAccount: null,
    entityHint: entityHintFromTags(tags),
  };
}

/**
 * @param {object} options
 * @param {string} options.id
 * @param {string} options.occurredDate
 * @param {string} options.account
 * @param {string} options.merchant
 * @param {number} options.amountMinor
 * @param {string} [options.monarchCategory]
 * @param {string} [options.notes]
 * @param {string[]} [options.tags]
 * @param {string} [options.owner]
 * @returns {RegisterLine}
 */
export function expectedLine(options) {
  const account = options.account;
  const tags = options.tags || ["e-doug"];
  return {
    id: options.id,
    occurredDate: options.occurredDate,
    account,
    accountLast4: accountLast4(account),
    accountType: inferAccountType(account),
    merchant: options.merchant,
    amountMinor: options.amountMinor,
    currency: "USD",
    monarchCategory: options.monarchCategory || "",
    originalStatement: "",
    notes: options.notes || "Expected movement: a recipe says this should exist, not yet observed.",
    tags,
    owner: options.owner || "Doug",
    pending: false,
    monarchReviewed: false,
    origin: "expected",
    state: "unreviewed",
    recipeId: "expected-movement",
    categoryId: options.categoryId || null,
    counterAccount: null,
    entityHint: entityHintFromTags(tags),
  };
}

/**
 * Expected movements shipped only with the synthetic fixture (never invented
 * on top of a real YA_MONARCH_CSV file).
 *
 * @param {RegisterLine[]} observations
 * @returns {RegisterLine[]}
 */
export function syntheticExpectedLines(observations) {
  /** @type {RegisterLine[]} */
  const extras = [];
  for (const line of observations) {
    if (!/payroll/i.test(line.merchant)) continue;
    const hsaMatch = line.notes.match(/HSA\s+\$?(\d+(?:\.\d{1,2})?)/i);
    if (!hsaMatch) continue;
    const [whole, frac = "00"] = hsaMatch[1].split(".");
    const minor = Number.parseInt(whole, 10) * 100 + Number.parseInt((frac + "00").slice(0, 2), 10);
    extras.push(
      expectedLine({
        id: `expected-hsa-${line.id}`,
        occurredDate: line.occurredDate,
        account: "HSA ••••9012",
        merchant: line.merchant,
        amountMinor: minor,
        monarchCategory: "HSA contribution",
        categoryId: "hsa-contribution",
        notes: `Expected movement: employee HSA counterpart for paycheck ${line.id}.`,
        tags: ["e-doug"],
      }),
    );
  }

  // Recurring bill the fixture does not yet observe for late October.
  extras.push(
    expectedLine({
      id: "expected-rent-2026-10",
      occurredDate: "2026-10-28",
      account: "Household Checking ••••4321",
      merchant: "Contoso Apartments",
      amountMinor: -185000,
      monarchCategory: "Rent",
      categoryId: "rent",
      notes: "Expected movement: recurring October rent not yet observed.",
      tags: ["e-household"],
    }),
  );
  extras.push(
    expectedLine({
      id: "expected-insurance-2026-10",
      occurredDate: "2026-10-12",
      account: "Doug Checking ••••1234",
      merchant: "Humongous Insurance",
      amountMinor: -14200,
      monarchCategory: "Insurance",
      categoryId: "insurance",
      notes: "Expected movement: recurring auto insurance draft not yet observed.",
      tags: ["e-doug"],
    }),
  );
  return extras;
}

/**
 * @param {import("./csv.mjs").CsvRecord[]} records
 * @param {{ includeSyntheticExpected?: boolean }} [options]
 * @returns {{ lines: RegisterLine[], observationCount: number, expectedCount: number }}
 */
export function toRegisterLines(records, options = {}) {
  const observations = records.map(observationFromRecord);
  const seen = new Set();
  for (const line of observations) {
    if (seen.has(line.id)) {
      throw new Error(`duplicate observation Id: ${line.id}`);
    }
    seen.add(line.id);
  }
  const expected = options.includeSyntheticExpected
    ? syntheticExpectedLines(observations)
    : [];
  const lines = observations.concat(expected).sort(compareLines);
  return {
    lines,
    observationCount: observations.length,
    expectedCount: expected.length,
  };
}

/**
 * @param {RegisterLine} a
 * @param {RegisterLine} b
 */
export function compareLines(a, b) {
  if (a.occurredDate !== b.occurredDate) {
    return a.occurredDate < b.occurredDate ? -1 : 1;
  }
  if (a.origin !== b.origin) {
    return a.origin === "expected" ? 1 : -1;
  }
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}
