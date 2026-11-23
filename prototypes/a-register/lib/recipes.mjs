/**
 * Toy recipe catalog and debit/credit preview for Prototype A.
 * Vocabulary follows the repo glossary: recipe, category, posting,
 * origin, review state, expected movement, contra-expense.
 */

import { previewBalance } from "./money.mjs";

/** @typedef {"unreviewed" | "interpreted" | "reconciled" | "voided"} ReviewState */
/** @typedef {"observed" | "expected"} Origin */
/** @typedef {"asset" | "liability" | "equity" | "income" | "expense"} AccountType */

/**
 * @typedef {object} Category
 * @property {string} id
 * @property {string} name
 * @property {"income" | "expense"} type
 * @property {string} entity
 */

/**
 * @typedef {object} Recipe
 * @property {string} id
 * @property {string} name
 * @property {string} description
 * @property {boolean} needsCategory
 * @property {boolean} needsCounterAccount
 * @property {"income" | "expense" | null} categoryType
 */

/**
 * Leaf categories only (glossary: only leaves take postings). Invented seed.
 * @type {readonly Category[]}
 */
export const CATEGORIES = Object.freeze([
  { id: "groceries", name: "Groceries", type: "expense", entity: "Household" },
  { id: "restaurants", name: "Restaurants", type: "expense", entity: "Household" },
  { id: "utilities", name: "Utilities", type: "expense", entity: "Household" },
  { id: "rent", name: "Rent", type: "expense", entity: "Household" },
  { id: "transportation", name: "Transportation", type: "expense", entity: "Household" },
  { id: "household-supplies", name: "Household supplies", type: "expense", entity: "Household" },
  { id: "entertainment", name: "Entertainment", type: "expense", entity: "Household" },
  { id: "insurance", name: "Insurance", type: "expense", entity: "Household" },
  { id: "medical", name: "Medical", type: "expense", entity: "Doug" },
  { id: "software", name: "Software", type: "expense", entity: "Doug" },
  { id: "payroll-taxes", name: "Payroll taxes", type: "expense", entity: "Doug" },
  { id: "hsa-contribution", name: "HSA contribution", type: "expense", entity: "Doug" },
  { id: "business-software", name: "Software", type: "expense", entity: "Dragonfly" },
  { id: "business-supplies", name: "Supplies", type: "expense", entity: "Dragonfly" },
  { id: "business-hosting", name: "Hosting", type: "expense", entity: "Dragonfly" },
  { id: "wages", name: "Wages", type: "income", entity: "Doug" },
  { id: "interest-income", name: "Interest income", type: "income", entity: "Doug" },
  { id: "client-revenue", name: "Client revenue", type: "income", entity: "Dragonfly" },
]);

/**
 * @type {readonly Recipe[]}
 */
export const RECIPES = Object.freeze([
  {
    id: "purchase",
    name: "Purchase",
    description: "Pay for an expense from a financial account.",
    needsCategory: true,
    needsCounterAccount: false,
    categoryType: "expense",
  },
  {
    id: "refund",
    name: "Refund (contra-expense)",
    description: "Credit the original expense category; not income.",
    needsCategory: true,
    needsCounterAccount: false,
    categoryType: "expense",
  },
  {
    id: "perk-credit",
    name: "Perk credit (contra-expense)",
    description: "Card perk or rebate against an expense; not income.",
    needsCategory: true,
    needsCounterAccount: false,
    categoryType: "expense",
  },
  {
    id: "income",
    name: "Income",
    description: "Earned or received inflow (wages, interest, revenue).",
    needsCategory: true,
    needsCounterAccount: false,
    categoryType: "income",
  },
  {
    id: "paycheck",
    name: "Paycheck",
    description: "Gross-to-net wages with tax and HSA withholding.",
    needsCategory: false,
    needsCounterAccount: false,
    categoryType: null,
  },
  {
    id: "card-payment",
    name: "Card payment",
    description: "Payment from a cash account to a card liability.",
    needsCategory: false,
    needsCounterAccount: true,
    categoryType: null,
  },
  {
    id: "transfer",
    name: "Own-account transfer",
    description: "Move money between two financial accounts on these books.",
    needsCategory: false,
    needsCounterAccount: true,
    categoryType: null,
  },
  {
    id: "reimbursement-claim",
    name: "Reimbursement claim",
    description: "Expense paid on behalf of a party; opens a receivable.",
    needsCategory: false,
    needsCounterAccount: false,
    categoryType: null,
  },
  {
    id: "expected-movement",
    name: "Expected movement",
    description: "A recipe says this movement should exist but is not yet observed.",
    needsCategory: true,
    needsCounterAccount: false,
    categoryType: null,
  },
]);

/** @type {Readonly<Record<string, ReviewState>>} */
export const REVIEW_STATES = Object.freeze({
  unreviewed: "unreviewed",
  interpreted: "interpreted",
  reconciled: "reconciled",
  voided: "voided",
});

/**
 * @param {string} id
 * @returns {Recipe | undefined}
 */
export function getRecipe(id) {
  return RECIPES.find((recipe) => recipe.id === id);
}

/**
 * @param {string} id
 * @returns {Category | undefined}
 */
export function getCategory(id) {
  return CATEGORIES.find((category) => category.id === id);
}

/**
 * Suggest a category from a Monarch category hint (display only; not applied).
 *
 * @param {string} hint
 * @returns {Category | undefined}
 */
export function suggestCategoryFromHint(hint) {
  const needle = (hint || "").trim().toLowerCase();
  if (!needle) return undefined;
  return CATEGORIES.find(
    (category) =>
      category.name.toLowerCase() === needle ||
      category.id === needle ||
      needle.includes(category.name.toLowerCase()),
  );
}

/**
 * @param {string} [accountName]
 * @returns {AccountType}
 */
export function inferAccountType(accountName = "") {
  const name = accountName.toLowerCase();
  if (/\b(visa|mastercard|amex|card)\b/.test(name)) return "liability";
  return "asset";
}

/**
 * @typedef {object} PreviewInput
 * @property {string} account
 * @property {number} amountMinor signed cents (negative = outflow from the feed)
 * @property {string} [merchant]
 * @property {string} [notes]
 * @property {string} [recipeId]
 * @property {string} [categoryId]
 * @property {string} [counterAccount]
 */

/**
 * Build a debit/credit preview for a line under a chosen recipe.
 * Amounts on postings are positive `amount_minor` (glossary).
 *
 * @param {PreviewInput} input
 * @returns {import("./money.mjs").BalancePreview}
 */
export function previewRecipe(input) {
  const recipe = input.recipeId ? getRecipe(input.recipeId) : undefined;
  if (!recipe) {
    return previewBalance([]);
  }
  const abs = Math.abs(input.amountMinor);
  const category = input.categoryId ? getCategory(input.categoryId) : undefined;
  const account = input.account || "Account";
  const counter = input.counterAccount || "";
  const accountType = inferAccountType(account);
  const counterType = counter ? inferAccountType(counter) : undefined;

  /** @type {import("./money.mjs").Posting[]} */
  let postings = [];

  switch (recipe.id) {
    case "purchase":
      if (!category) break;
      postings = [
        posting(categoryLabel(category), "debit", abs, category.type),
        posting(account, "credit", abs, accountType),
      ];
      break;
    case "refund":
    case "perk-credit":
      if (!category) break;
      postings = [
        posting(account, "debit", abs, accountType),
        posting(categoryLabel(category), "credit", abs, category.type),
      ];
      break;
    case "income":
      if (!category) break;
      postings = [
        posting(account, "debit", abs, accountType),
        posting(categoryLabel(category), "credit", abs, category.type),
      ];
      break;
    case "paycheck": {
      // Toy gross-to-net: net is the observed amount; withhold 8% tax and $200 HSA
      // when the net is large enough, otherwise just net pay vs wages.
      const hsa = abs >= 50_000 ? 20_000 : 0;
      const tax = abs >= 50_000 ? Number((BigInt(abs) * 8n) / 100n) : 0;
      const gross = abs + hsa + tax;
      postings = [
        posting(account, "debit", abs, "asset"),
      ];
      if (hsa > 0) {
        postings.push(posting("HSA (employee withholding)", "debit", hsa, "asset"));
      }
      if (tax > 0) {
        postings.push(posting("Payroll taxes", "debit", tax, "expense"));
      }
      postings.push(posting("Wages", "credit", gross, "income"));
      break;
    }
    case "card-payment":
    case "transfer":
      if (!counter) break;
      if (input.amountMinor < 0) {
        postings = [
          posting(counter, "debit", abs, counterType),
          posting(account, "credit", abs, accountType),
        ];
      } else {
        postings = [
          posting(account, "debit", abs, accountType),
          posting(counter, "credit", abs, counterType),
        ];
      }
      break;
    case "reimbursement-claim":
      if (input.amountMinor < 0) {
        postings = [
          posting("Reimbursement claim (receivable)", "debit", abs, "asset"),
          posting(account, "credit", abs, accountType),
        ];
      } else {
        postings = [
          posting(account, "debit", abs, accountType),
          posting("Reimbursement claim (receivable)", "credit", abs, "asset"),
        ];
      }
      break;
    case "expected-movement":
      if (!category) break;
      if (category.type === "expense") {
        postings = [
          posting(categoryLabel(category), "debit", abs, category.type),
          posting(account, "credit", abs, accountType),
        ];
      } else {
        postings = [
          posting(account, "debit", abs, accountType),
          posting(categoryLabel(category), "credit", abs, category.type),
        ];
      }
      break;
    default:
      break;
  }

  return previewBalance(postings);
}

/**
 * @param {string} account
 * @param {"debit" | "credit"} side
 * @param {number} amountMinor
 * @param {string} [accountType]
 * @returns {import("./money.mjs").Posting}
 */
function posting(account, side, amountMinor, accountType) {
  return { account, side, amountMinor, accountType };
}

/**
 * @param {Category} category
 */
function categoryLabel(category) {
  return `${category.name} (${category.entity})`;
}

/**
 * After a recipe is chosen the line becomes Interpreted (draft).
 * Accept/post moves a balanced Interpreted line to Reconciled.
 *
 * @param {ReviewState} current
 * @param {"choose" | "clear" | "accept" | "void" | "reopen"} action
 * @param {{ balanced?: boolean, hasRecipe?: boolean }} [ctx]
 * @returns {ReviewState}
 */
export function nextReviewState(current, action, ctx = {}) {
  if (action === "void") return "voided";
  if (action === "reopen") return ctx.hasRecipe ? "interpreted" : "unreviewed";
  if (action === "clear") return current === "voided" ? current : "unreviewed";
  if (action === "choose") {
    if (current === "voided" || current === "reconciled") return current;
    return "interpreted";
  }
  if (action === "accept") {
    if (current === "voided") return current;
    if (ctx.balanced && ctx.hasRecipe) return "reconciled";
    return current === "unreviewed" && ctx.hasRecipe ? "interpreted" : current;
  }
  return current;
}
