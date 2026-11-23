import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { formatCents, parseAmountToMinor, previewBalance } from "../lib/money.mjs";
import { previewRecipe } from "../lib/recipes.mjs";

describe("parseAmountToMinor", () => {
  it("parses signed decimals and grouping without floats", () => {
    assert.equal(parseAmountToMinor("12.34"), 1234);
    assert.equal(parseAmountToMinor("-12.34"), -1234);
    assert.equal(parseAmountToMinor("$1,234.56"), 123456);
    assert.equal(parseAmountToMinor("(12.34)"), -1234);
    assert.equal(parseAmountToMinor("12"), 1200);
    assert.equal(parseAmountToMinor("12.3"), 1230);
    assert.equal(parseAmountToMinor("0.86"), 86);
  });

  it("rejects malformed amounts", () => {
    assert.throws(() => parseAmountToMinor("12.345"), /invalid amount/);
    assert.throws(() => parseAmountToMinor(""), /empty amount/);
    assert.throws(() => parseAmountToMinor("12.3.4"), /invalid amount/);
  });
});

describe("formatCents", () => {
  it("formats integer cents with a grouping separator", () => {
    assert.equal(formatCents(1234), "$12.34");
    assert.equal(formatCents(-1234), "-$12.34");
    assert.equal(formatCents(314256), "$3,142.56");
    assert.equal(formatCents(0), "$0.00");
  });
});

describe("previewBalance", () => {
  it("reports balanced when debit cents equal credit cents", () => {
    const preview = previewBalance([
      { account: "Groceries", side: "debit", amountMinor: 12847 },
      { account: "Checking", side: "credit", amountMinor: 12847 },
    ]);
    assert.equal(preview.debits, 12847);
    assert.equal(preview.credits, 12847);
    assert.equal(preview.balanced, true);
    assert.equal(preview.residual, 0);
  });

  it("reports the residual when unbalanced", () => {
    const preview = previewBalance([
      { account: "Groceries", side: "debit", amountMinor: 100 },
      { account: "Checking", side: "credit", amountMinor: 40 },
    ]);
    assert.equal(preview.balanced, false);
    assert.equal(preview.residual, 60);
  });

  it("rejects a float or negative amount_minor", () => {
    assert.throws(
      () => previewBalance([{ account: "A", side: "debit", amountMinor: 1.5 }]),
      /non-negative integer/,
    );
    assert.throws(
      () => previewBalance([{ account: "A", side: "debit", amountMinor: -1 }]),
      /non-negative integer/,
    );
  });
});

describe("previewRecipe", () => {
  it("balances a purchase: debit category, credit account", () => {
    const preview = previewRecipe({
      account: "Household Visa ••••5678",
      amountMinor: -1480,
      recipeId: "purchase",
      categoryId: "restaurants",
    });
    assert.equal(preview.balanced, true);
    assert.equal(preview.debits, 1480);
    assert.equal(preview.credits, 1480);
    assert.equal(preview.postings[0].side, "debit");
    assert.equal(preview.postings[1].side, "credit");
  });

  it("treats a perk credit as a contra-expense, not income", () => {
    const preview = previewRecipe({
      account: "Household Visa ••••5678",
      amountMinor: 500,
      recipeId: "perk-credit",
      categoryId: "groceries",
    });
    assert.equal(preview.balanced, true);
    assert.ok(preview.postings.some((p) => p.account.startsWith("Groceries") && p.side === "credit"));
    assert.ok(!preview.postings.some((p) => /income/i.test(p.account)));
  });

  it("balances a paycheck gross-to-net split in integer cents", () => {
    const preview = previewRecipe({
      account: "Doug Checking ••••1234",
      amountMinor: 314256,
      recipeId: "paycheck",
    });
    assert.equal(preview.balanced, true);
    assert.equal(preview.debits, preview.credits);
    const hsa = preview.postings.find((p) => p.account.startsWith("HSA"));
    assert.equal(hsa.amountMinor, 20000);
  });

  it("is incomplete (empty) until a required category is chosen", () => {
    const preview = previewRecipe({
      account: "Doug Checking ••••1234",
      amountMinor: -1200,
      recipeId: "purchase",
    });
    assert.equal(preview.postings.length, 0);
    assert.equal(preview.balanced, true);
    assert.equal(preview.debits, 0);
  });
});
