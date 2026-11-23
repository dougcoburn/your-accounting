/**
 * Integer minor-unit (cents) money helpers. Never use floats for amounts.
 */

/**
 * Parse a decimal amount string into signed integer cents.
 * Accepts `12.34`, `-12.34`, `$1,234.56`, `(12.34)`, and whole dollars.
 *
 * @param {string | number} value
 * @returns {number}
 */
export function parseAmountToMinor(value) {
  if (typeof value === "number") {
    if (!Number.isInteger(value)) {
      throw new RangeError("numeric amounts must already be integer cents");
    }
    return value;
  }
  if (typeof value !== "string") {
    throw new TypeError("parseAmountToMinor expects a string");
  }
  const trimmed = value.trim();
  if (trimmed === "") {
    throw new SyntaxError("empty amount");
  }
  const negative =
    trimmed.startsWith("-") ||
    (trimmed.startsWith("(") && trimmed.endsWith(")"));
  const cleaned = trimmed
    .replace(/^\(/, "")
    .replace(/\)$/, "")
    .replace(/^[+$€£-]+/g, "")
    .replace(/,/g, "")
    .trim();
  if (!/^\d+(\.\d{0,2})?$/.test(cleaned)) {
    throw new SyntaxError(`invalid amount: ${value}`);
  }
  const [wholePart, fracPart = ""] = cleaned.split(".");
  const whole = Number.parseInt(wholePart || "0", 10);
  const frac = Number.parseInt((fracPart + "00").slice(0, 2), 10);
  const minor = whole * 100 + frac;
  if (!Number.isSafeInteger(minor)) {
    throw new RangeError("amount exceeds safe integer cents");
  }
  return negative ? -minor : minor;
}

/**
 * Format signed integer cents as `$1,234.56` (negative amounts keep a minus).
 *
 * @param {number} minor
 * @returns {string}
 */
export function formatCents(minor) {
  if (!Number.isInteger(minor)) {
    throw new TypeError("formatCents expects integer cents");
  }
  const sign = minor < 0 ? "-" : "";
  const abs = Math.abs(minor);
  const dollars = Math.trunc(abs / 100);
  const cents = String(abs % 100).padStart(2, "0");
  const withGroups = String(dollars).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${sign}$${withGroups}.${cents}`;
}

/**
 * @typedef {object} Posting
 * @property {string} account
 * @property {"debit" | "credit"} side
 * @property {number} amountMinor positive integer cents
 * @property {string} [accountType]
 */

/**
 * @typedef {object} BalancePreview
 * @property {number} debits
 * @property {number} credits
 * @property {boolean} balanced
 * @property {number} residual debits minus credits (0 when balanced)
 * @property {Posting[]} postings
 */

/**
 * Sum debit and credit postings in integer cents.
 *
 * @param {Posting[]} postings
 * @returns {BalancePreview}
 */
export function previewBalance(postings) {
  if (!Array.isArray(postings)) {
    throw new TypeError("previewBalance expects an array");
  }
  let debits = 0;
  let credits = 0;
  for (const posting of postings) {
    if (!Number.isInteger(posting.amountMinor) || posting.amountMinor < 0) {
      throw new RangeError("posting amount_minor must be a non-negative integer");
    }
    if (posting.side === "debit") debits += posting.amountMinor;
    else if (posting.side === "credit") credits += posting.amountMinor;
    else throw new RangeError(`invalid posting side: ${posting.side}`);
  }
  return {
    debits,
    credits,
    balanced: debits === credits,
    residual: debits - credits,
    postings,
  };
}

/**
 * @param {number} a
 * @param {number} b
 * @returns {number}
 */
export function sumMinor(a, b) {
  if (!Number.isInteger(a) || !Number.isInteger(b)) {
    throw new TypeError("sumMinor expects integer cents");
  }
  const result = a + b;
  if (!Number.isSafeInteger(result)) {
    throw new RangeError("sum exceeds safe integer cents");
  }
  return result;
}
