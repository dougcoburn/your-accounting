import type {
  Observation,
  RecipeId,
  SettlementGroupSuggestion,
  SettlementParticipant,
  Treatment,
} from "./types.ts";

const CARD_PAYMENT_WINDOW_DAYS = 5;
const TRANSFER_WINDOW_DAYS = 3;
const HSA_WINDOW_DAYS = 10;

/** Canned Northwind Staffing stub used by the synthetic fixture and tests. */
export const NORTHWIND_STUB = {
  merchantPattern: /northwind staffing/i,
  grossMinor: 600_000,
  taxesMinor: 100_000,
  pretaxMinor: 55_000,
  rothMinor: 20_000,
  hsaMinor: 15_000,
  netMinor: 410_000,
} as const;

export function daysBetween(a: string, b: string): number {
  const ms = Date.parse(`${a}T00:00:00Z`) - Date.parse(`${b}T00:00:00Z`);
  return Math.abs(ms) / 86_400_000;
}

export function isCardAccount(account: string): boolean {
  return /\b(card|visa|mastercard|amex|credit)\b/i.test(account);
}

export function isHsaAccount(account: string): boolean {
  return /\bhsa\b/i.test(account);
}

export function isDepositAccount(account: string): boolean {
  return /\b(checking|savings|bank|money market|hsa)\b/i.test(account);
}

export function isCardPaymentHint(obs: Observation): boolean {
  const blob = `${obs.category} ${obs.merchant} ${obs.originalStatement}`;
  return /credit card payment|card payment|payment[\s-]*thank you|autopay/i.test(blob);
}

export function isTransferHint(obs: Observation): boolean {
  return /transfer/i.test(`${obs.category} ${obs.merchant}`);
}

export function isPaycheckHint(obs: Observation): boolean {
  const blob = `${obs.category} ${obs.merchant} ${obs.originalStatement}`;
  return obs.amountMinor > 0 && /paycheck|payroll|salary|direct deposit|staffing/i.test(blob);
}

export function isRefundHint(obs: Observation): boolean {
  const blob = `${obs.category} ${obs.merchant} ${obs.originalStatement}`;
  return obs.amountMinor > 0 && /refund/i.test(blob);
}

export function isPerkHint(obs: Observation): boolean {
  const blob = `${obs.category} ${obs.merchant} ${obs.originalStatement}`;
  return obs.amountMinor > 0 && /reward|perk|cash ?back|statement credit/i.test(blob);
}

export function defaultExpenseCategory(obs: Observation): string {
  return obs.category.trim() || "Uncategorized";
}

function byId(a: Observation, b: Observation): number {
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

function counterpartScore(base: Observation, candidate: Observation): number {
  // Lower is better. Date distance first, then stable id.
  return daysBetween(base.date, candidate.date) * 1000 + (candidate.id < base.id ? 0 : 1);
}

export function findCardPaymentCounterpart(
  obs: Observation,
  all: Observation[],
  windowDays = CARD_PAYMENT_WINDOW_DAYS,
): Observation | undefined {
  const want = -obs.amountMinor;
  const candidates = all
    .filter((other) => {
      if (other.id === obs.id) return false;
      if (other.amountMinor !== want) return false;
      if (other.account === obs.account) return false;
      if (daysBetween(obs.date, other.date) > windowDays) return false;
      const involved = isCardPaymentHint(obs) || isCardPaymentHint(other);
      if (!involved) return false;
      // Prefer a card account paired with a deposit account.
      const cardSide = isCardAccount(obs.account) || isCardAccount(other.account);
      const bankSide = isDepositAccount(obs.account) || isDepositAccount(other.account);
      return cardSide && bankSide;
    })
    .sort((a, b) => counterpartScore(obs, a) - counterpartScore(obs, b) || byId(a, b));
  return candidates[0];
}

export function findTransferCounterpart(
  obs: Observation,
  all: Observation[],
  windowDays = TRANSFER_WINDOW_DAYS,
): Observation | undefined {
  const want = -obs.amountMinor;
  const candidates = all
    .filter((other) => {
      if (other.id === obs.id) return false;
      if (other.amountMinor !== want) return false;
      if (other.account === obs.account) return false;
      if (daysBetween(obs.date, other.date) > windowDays) return false;
      return isTransferHint(obs) || isTransferHint(other);
    })
    .sort((a, b) => counterpartScore(obs, a) - counterpartScore(obs, b) || byId(a, b));
  return candidates[0];
}

export type PaycheckSplit = {
  grossMinor: number;
  taxesMinor: number;
  pretaxMinor: number;
  rothMinor: number;
  hsaMinor: number;
  netMinor: number;
};

/**
 * Known stub for Northwind Staffing; otherwise a balanced estimate from net
 * using fixed ratios that sum with net to gross.
 */
export function paycheckSplitFor(obs: Observation): PaycheckSplit {
  if (NORTHWIND_STUB.merchantPattern.test(obs.merchant) && obs.amountMinor === NORTHWIND_STUB.netMinor) {
    return { ...NORTHWIND_STUB };
  }
  const netMinor = obs.amountMinor;
  // taxes 20%, pre-tax 5%, Roth 3%, HSA 2.5% of gross; net is the remainder (69.5%).
  const grossMinor = Math.round(netMinor / 0.695);
  const taxesMinor = Math.round(grossMinor * 0.2);
  const pretaxMinor = Math.round(grossMinor * 0.05);
  const rothMinor = Math.round(grossMinor * 0.03);
  let hsaMinor = Math.round(grossMinor * 0.025);
  const remainder = grossMinor - netMinor - taxesMinor - pretaxMinor - rothMinor - hsaMinor;
  hsaMinor += remainder;
  return { grossMinor, taxesMinor, pretaxMinor, rothMinor, hsaMinor, netMinor };
}

export function findExpectedHsa(
  obs: Observation,
  all: Observation[],
  hsaMinor: number,
  windowDays = HSA_WINDOW_DAYS,
): Observation | undefined {
  if (hsaMinor <= 0) return undefined;
  const candidates = all
    .filter((other) => {
      if (other.id === obs.id) return false;
      if (other.amountMinor !== hsaMinor) return false;
      if (!isHsaAccount(other.account)) return false;
      if (other.amountMinor <= 0) return false;
      return daysBetween(obs.date, other.date) <= windowDays;
    })
    .sort((a, b) => counterpartScore(obs, a) - counterpartScore(obs, b) || byId(a, b));
  return candidates[0];
}

function participant(partial: SettlementParticipant): SettlementParticipant {
  return partial;
}

function groupResidual(participants: SettlementParticipant[], recipeId: RecipeId): number {
  // Residual is unused observed cash vs implied cookbook amounts.
  // For pairing recipes, residual is the signed sum of observed legs.
  if (recipeId === "card-payment" || recipeId === "transfer") {
    return participants
      .filter((p) => p.origin === "observed" && p.filled)
      .reduce((s, p) => s + p.amountMinor, 0);
  }
  return 0;
}

export function cardPaymentGroup(obs: Observation, counterpart: Observation | undefined): SettlementGroupSuggestion {
  const outflow = obs.amountMinor < 0 ? obs : counterpart?.amountMinor && counterpart.amountMinor < 0 ? counterpart : obs;
  const inflow = obs.amountMinor > 0 ? obs : counterpart?.amountMinor && counterpart.amountMinor > 0 ? counterpart : undefined;
  const bank = [obs, counterpart].find((x) => x && isDepositAccount(x.account) && !isCardAccount(x.account));
  const card = [obs, counterpart].find((x) => x && isCardAccount(x.account));
  const bankObs = bank ?? (outflow.amountMinor < 0 ? outflow : undefined);
  const cardObs = card ?? inflow;

  const participants: SettlementParticipant[] = [
    participant({
      role: "bank-outflow",
      label: bankObs ? `${bankObs.account} · ${bankObs.merchant}` : "Bank outflow",
      origin: "observed",
      filled: Boolean(bankObs),
      amountMinor: bankObs?.amountMinor ?? -Math.abs(obs.amountMinor),
      account: bankObs?.account ?? "",
      observationId: bankObs?.id,
    }),
    participant({
      role: "card-payment",
      label: cardObs ? `${cardObs.account} · ${cardObs.merchant}` : "Card payment (unfilled)",
      origin: cardObs ? "observed" : "expected",
      filled: Boolean(cardObs),
      amountMinor: cardObs?.amountMinor ?? Math.abs(obs.amountMinor),
      account: cardObs?.account ?? "",
      observationId: cardObs?.id,
    }),
  ];
  const complete = participants.every((p) => p.filled);
  return {
    title: "Card payment",
    recipeId: "card-payment",
    participants,
    complete,
    residualMinor: groupResidual(participants, "card-payment"),
    note: complete
      ? "Pairs the card payment with the matching bank outflow."
      : "Matching card or bank leg was not found; the group stays incomplete.",
  };
}

export function transferGroup(obs: Observation, counterpart: Observation | undefined): SettlementGroupSuggestion {
  const legs = [obs, counterpart].filter((x): x is Observation => Boolean(x));
  legs.sort((a, b) => a.amountMinor - b.amountMinor || byId(a, b));
  const participants: SettlementParticipant[] = legs.map((leg) =>
    participant({
      role: leg.amountMinor < 0 ? "transfer-out" : "transfer-in",
      label: `${leg.account} · ${leg.merchant}`,
      origin: "observed",
      filled: true,
      amountMinor: leg.amountMinor,
      account: leg.account,
      observationId: leg.id,
    }),
  );
  if (!counterpart) {
    participants.push(
      participant({
        role: obs.amountMinor < 0 ? "transfer-in" : "transfer-out",
        label: "Counterpart transfer (unfilled)",
        origin: "expected",
        filled: false,
        amountMinor: -obs.amountMinor,
        account: "",
      }),
    );
  }
  const complete = participants.every((p) => p.filled);
  return {
    title: "Own-account transfer",
    recipeId: "transfer",
    participants,
    complete,
    residualMinor: groupResidual(participants, "transfer"),
    note: complete ? "Both legs of the transfer are present." : "The other transfer leg was not found.",
  };
}

export function paycheckGroup(
  obs: Observation,
  split: PaycheckSplit,
  hsaObs: Observation | undefined,
): SettlementGroupSuggestion {
  const participants: SettlementParticipant[] = [
    participant({
      role: "gross",
      label: "Gross wages",
      origin: "expected",
      filled: true,
      amountMinor: split.grossMinor,
      account: "Income · Wages",
    }),
    participant({
      role: "taxes",
      label: "Taxes",
      origin: "expected",
      filled: true,
      amountMinor: -split.taxesMinor,
      account: "Expense · Payroll taxes",
    }),
    participant({
      role: "pre-tax",
      label: "Pre-tax (401k + health)",
      origin: "expected",
      filled: true,
      amountMinor: -split.pretaxMinor,
      account: "Retirement · Pre-tax",
    }),
    participant({
      role: "roth",
      label: "Roth 401k",
      origin: "expected",
      filled: true,
      amountMinor: -split.rothMinor,
      account: "Retirement · Roth",
    }),
    participant({
      role: "hsa",
      label: "HSA employee deduction",
      origin: "expected",
      filled: true,
      amountMinor: -split.hsaMinor,
      account: "Summit HSA",
    }),
    participant({
      role: "net",
      label: `${obs.account} · net pay`,
      origin: "observed",
      filled: true,
      amountMinor: split.netMinor,
      account: obs.account,
      observationId: obs.id,
    }),
    participant({
      role: "expected-hsa",
      label: hsaObs ? `${hsaObs.account} · ${hsaObs.merchant}` : "Expected HSA",
      origin: hsaObs ? "observed" : "expected",
      filled: Boolean(hsaObs),
      amountMinor: split.hsaMinor,
      account: hsaObs?.account ?? "Summit HSA",
      observationId: hsaObs?.id,
    }),
  ];
  // The paycheck group itself is complete without the Expected HSA slot,
  // which is its own draft group (ADR 0006 / month-1 plan).
  return {
    title: "Paycheck",
    recipeId: "paycheck",
    participants,
    complete: true,
    residualMinor: split.grossMinor - split.taxesMinor - split.pretaxMinor - split.rothMinor - split.hsaMinor - split.netMinor,
    note: hsaObs
      ? "Expected HSA is filled by a matching HSA inflow."
      : "Expected HSA can sit unfilled as its own draft group.",
  };
}

function recipeLabel(id: RecipeId): string {
  switch (id) {
    case "purchase":
      return "Purchase";
    case "refund":
      return "Refund (contra-expense)";
    case "perk-credit":
      return "Perk credit";
    case "card-payment":
      return "Card payment";
    case "transfer":
      return "Own-account transfer";
    case "paycheck":
      return "Paycheck";
  }
}

export function suggestTreatment(obs: Observation, all: Observation[]): Treatment {
  if (isPaycheckHint(obs)) {
    const split = paycheckSplitFor(obs);
    const hsa = findExpectedHsa(obs, all, split.hsaMinor);
    return {
      recipeId: "paycheck",
      recipeLabel: recipeLabel("paycheck"),
      category: "Income · Wages",
      summary: `Paycheck · gross ${split.grossMinor / 100} → net on ${obs.account}`,
      settlementGroup: paycheckGroup(obs, split, hsa),
    };
  }

  if (isCardPaymentHint(obs)) {
    const counterpart = findCardPaymentCounterpart(obs, all);
    const group = cardPaymentGroup(obs, counterpart);
    return {
      recipeId: "card-payment",
      recipeLabel: recipeLabel("card-payment"),
      category: "Transfer · Credit card payment",
      summary: counterpart
        ? `Card payment · pair with ${counterpart.account}`
        : "Card payment · counterpart not found",
      settlementGroup: group,
    };
  }

  if (isTransferHint(obs)) {
    const counterpart = findTransferCounterpart(obs, all);
    return {
      recipeId: "transfer",
      recipeLabel: recipeLabel("transfer"),
      category: "Transfer",
      summary: counterpart ? `Transfer · pair with ${counterpart.account}` : "Transfer · counterpart not found",
      settlementGroup: transferGroup(obs, counterpart),
    };
  }

  if (isRefundHint(obs)) {
    const category = defaultExpenseCategory(obs);
    return {
      recipeId: "refund",
      recipeLabel: recipeLabel("refund"),
      category,
      summary: `Refund · contra-expense to ${category}`,
    };
  }

  if (isPerkHint(obs)) {
    return {
      recipeId: "perk-credit",
      recipeLabel: recipeLabel("perk-credit"),
      category: "Expense · Card perks",
      summary: "Perk credit · contra-expense (not income)",
    };
  }

  if (obs.amountMinor < 0) {
    const category = defaultExpenseCategory(obs);
    return {
      recipeId: "purchase",
      recipeLabel: recipeLabel("purchase"),
      category,
      summary: `Purchase · ${category}`,
    };
  }

  const category = defaultExpenseCategory(obs);
  return {
    recipeId: "purchase",
    recipeLabel: recipeLabel("purchase"),
    category,
    summary: `Unclassified inflow · ${category}`,
  };
}

/** Deterministic map of id → treatment, computed once over the full set. */
export function suggestAll(all: Observation[]): Map<string, Treatment> {
  const sorted = [...all].sort(byId);
  const map = new Map<string, Treatment>();
  for (const obs of sorted) {
    map.set(obs.id, suggestTreatment(obs, sorted));
  }
  return map;
}
