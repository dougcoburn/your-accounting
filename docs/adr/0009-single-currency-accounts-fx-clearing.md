# ADR 0009: Single-currency accounts; one multi-currency FX Clearing account

Status: Accepted (2026-11-26, Doug). Post-month-1 unit: it is not part of the month-1 implementation plan, and no month-1 unit depends on it. Until it is scheduled and lands, schema v2 and the kernel interface spec stay as they are.

## Context
Schema v2 puts a currency on every posting and balances every event per currency (INV-001, ADR 0008 amendment). A currency is fixed only on financial accounts (INV-004). A ledger account that is not backed by a financial account, such as a receivable, a category or a clearing account, can hold postings in any currency, and SPEC §2.3 calls categories "independent of currency".

That leaves two things open: SPEC §13 decision 2 (the journal model for an FX exchange) and decision 6 (reporting across currencies). An exchange from EUR to USD must balance EUR and USD separately. Something has to receive the EUR and give the USD, and it must not create a fictitious gain or a balance that only means something after conversion.

## Decision (Doug, 2026-11-26)
1. **Accounts are single-currency.** Currency is a property of the ledger account. Every journal line must be in its account's currency. The kernel rejects any other line.
2. **One exception: the FX Clearing account.** It is a special multi-currency ledger account that the kernel treats as exceptional. It is the only account whose lines may be in more than one currency.
3. **FX Clearing shape per transaction.** In each accounting event that touches FX Clearing, the FX Clearing rows have **exactly one debit currency and exactly one credit currency**. A side may have several rows in its one currency.
4. **Derived rate.** For that event, the rate is the FX Clearing credit total divided by the FX Clearing debit total: units of the credit currency per unit of the debit currency.
5. **Effective balance is zero.** Every event leaves FX Clearing's effective balance at zero: the debit total converted at the event's own derived rate equals the credit total. Raw per-currency sums of FX Clearing are never reported as a balance. Reports show FX Clearing as a list of closed groups with their rates.
6. **INV-001 still holds.** Debits equal credits per event per currency across the whole event. The bank or cash legs balance each currency against the FX Clearing rows.
7. **No month-end revaluation and no single reporting currency.** Each currency is reported on its own at month-end. Nothing is translated or revalued into a home currency, and no FX gain or loss is journaled for it.
8. **Rate plausibility check.** An external, doctor-style check flags derived rates that are implausible for the exchange date. The zero effective balance is true by construction, so this check is what catches a wrong amount.
9. **Fees go to an expense account,** normally not through FX Clearing, so the derived rate reflects the exchange itself.

## Terms
- **FX group (exchange transaction):** one accounting event that has FX Clearing rows. It is closed when it is committed, because the kernel only accepts the shape in decision 3.
- **Debit total D, currency A:** the sum of the event's FX Clearing debit rows, all in currency A.
- **Credit total C, currency B:** the sum of the event's FX Clearing credit rows, all in currency B, and B is not A.
- **Derived rate r:** C / D, in B per A, after the minor-unit adjustment below.
- **Effective balance** of FX Clearing after an event: D x r - C, in B. It is zero for every accepted event, so the account's effective balance is always zero.

## What the kernel enforces
The kernel never stores or computes a rate. It enforces three rules, and the effective-zero invariant follows from them.

1. **Account-currency match.** A posting's currency equals its ledger account's currency, unless the account is the FX Clearing account. This replaces INV-005's narrower rule (only financial-account-backed ledger accounts) and is the same error, `CurrencyMismatch`.
2. **Ledger account currency is consistent.** Every ledger account except FX Clearing has a currency. FX Clearing has none. A ledger account backed by a financial account has that financial account's currency.
3. **FX Clearing shape, per event.** If an event has any FX Clearing rows, then:
   - its FX Clearing debit rows are all in one currency A, and there is at least one;
   - its FX Clearing credit rows are all in one currency B, and there is at least one;
   - A is not B.

   Posting magnitudes are already positive (INV-002), so both totals are nonzero whenever both sides exist.

**Why that is enough.** With D > 0 and C > 0 the rate r = C / D is defined, and D x r - C = 0 exactly, for every accepted event. The effective balance is the sum of those per-event zeros. So the kernel does not need to test the invariant numerically and never needs exchange-rate data. The only ways to break it are a missing side (r is 0 or undefined), two currencies on one side (no single rate), or one currency on both sides (no exchange). Rule 3 rejects each of these.

**What the kernel cannot check.** Whether r is the right rate. An event that books EUR 1,000.00 against USD 11,850.00 is shaped correctly and is accepted. That is what the plausibility check is for (decision 8).

### Rejections (kernel interface spec §4.3 style)
These land in `docs/KERNEL_INTERFACE_SPEC.md` §4.3 in the same change as the schema, as §7 requires. Until then they are proposed.

| Rule | RAISE substring (proposed) | Code | If the seal raises it after U19 accepted |
|---|---|---|---|
| Posting currency is not its ledger account's (rule 1) | `posting currency must match` (existing substring; message widened to "the ledger account currency") | `CurrencyMismatch` (existing, class user) | user error (trigger, not a seal check) |
| Financial-account-backed ledger account in another currency (rule 2) | `ledger account currency must match its financial account` | `CurrencyMismatch` | user error |
| Currency missing on an ordinary account, or set on FX Clearing (rule 2) | CHECK on `ledger_accounts.currency` and `kind` | `InvalidShape` token `account_currency` (new). This row must sit above the generic "CHECK text containing `currency`" row, or it maps to `InvalidCurrency` | user error |
| FX Clearing rows with a missing side, two currencies on one side, or the same currency on both sides (rule 3) | `ADR 0009: FX Clearing rows need exactly one debit currency and one different credit currency` | `InvalidShape` token `fx_clearing` (new), refs `ADR 0009`, ids the event | `ValidatorMissed` (U19 rejects first) |
| A second FX Clearing account, if Doug keeps one per entity (open question 1) | partial UNIQUE index on `kind = 'fx_clearing'` | `InvalidShape` token `unique_other` (existing) | user error |

`InvalidCurrency` stays the format check (three uppercase letters). No new top-level code is needed.

## Schema representation (proposed delta, not yet in `docs/schema-v2.sql`)
- `ledger_accounts.currency TEXT`, same format CHECK as other currency columns, with `CHECK ((kind = 'fx_clearing') = (currency IS NULL))`.
- `ledger_accounts.kind` gains `fx_clearing`. It is distinct from `clearing`, which stays the ADR 0006 settlement clearing kind with its per-group zero residual (INV-008). `CHECK (kind <> 'fx_clearing' OR (account_type = 'asset' AND financial_account_id IS NULL AND parent_id IS NULL))`. The account type hardly matters, because the effective balance is always zero; asset matches the other clearing kind.
- Partial unique index: at most one `fx_clearing` account per database (pending open question 1).
- Trigger: a financial-account-backed ledger account's `currency` equals `financial_accounts.currency`.
- `journal_postings_currency_valid` changes from "match the financial account's currency, if any" to "match `ledger_accounts.currency`, unless the account is `fx_clearing`".
- Category tree: a child category has its parent's currency (`ledger_accounts_parent_valid`, `CategoryParentInvalid`), if open question 2 confirms categories carry a currency.
- `commits_seal` gains the rule 3 clause per event in the commit. A matching `doctor_fx_clearing_shape` view lets `doctor` re-check sealed history (`DoctorDrift` on a database the kernel sealed).
- Read models: `account_balances` leaves out `fx_clearing` rows. A new view `fx_clearing_groups(event_id, effective_date, debit_currency, debit_minor, credit_currency, credit_minor, reverses_id)` lists the groups. The rate is computed outside SQL, because it needs the minor-unit table below.
- Smoke checks for each accept and reject path above (§7 item 3).

## Worked example (synthetic)
Tx 15, effective date 2026-03-15 (invented). An entity sells EUR 1,000.00 from its EUR checking account. The provider pays USD in two tranches, 1,080.00 and 105.00, and withholds a 15.00 fee from the second, so the second deposit is 90.00. The fee goes to a USD expense account, not to FX Clearing. All accounts, ids and amounts are invented.

| # | Ledger account (account currency) | Side | Amount | `amount_minor` |
|---|---|---|---|---|
| 1 | EUR checking (EUR) | Credit | EUR 1,000.00 | 100000 |
| 2 | FX Clearing (none) | Debit | EUR 1,000.00 | 100000 |
| 3 | FX Clearing (none) | Credit | USD 1,080.00 | 108000 |
| 4 | FX Clearing (none) | Credit | USD 105.00 | 10500 |
| 5 | USD checking (USD) | Debit | USD 1,080.00 | 108000 |
| 6 | USD checking (USD) | Debit | USD 90.00 | 9000 |
| 7 | Bank fees, expense leaf (USD) | Debit | USD 15.00 | 1500 |

**INV-001, per currency:**
- EUR: debits 100000 (row 2) = credits 100000 (row 1).
- USD: debits 108000 + 9000 + 1500 = 118500 (rows 5, 6, 7) = credits 108000 + 10500 = 118500 (rows 3, 4).

**Shape (rule 3):** FX Clearing debits are all EUR (one row), credits are all USD (two rows), and EUR is not USD. Accepted.

**Derived rate:** D = EUR 1,000.00, C = USD 1,185.00. r = 1185.00 / 1000.00 = **1.185 USD per EUR** exactly. In minor units: 118500 / 100000 x 10^(2 - 2) = 1.185.

**Effective balance:** 1,000.00 x 1.185 - 1,185.00 = 0.

**Why the fee goes to expense:** had the fee been netted inside FX Clearing (row 4 as a USD 90.00 credit and no row 7), C would be 1,170.00 and r would be 1.170. That still balances and still has a zero effective balance, but the rate would understate the exchange by 0.015 and the fee would vanish from expenses. The kernel cannot tell the two apart; recipes book fees to expense, and the plausibility check may flag the low rate.

**Rejected variants (synthetic):**
- Row 7 posted to a EUR fee account in USD: `CurrencyMismatch`.
- An extra FX Clearing credit of GBP 10.00 (balanced by a GBP account): two credit currencies, `InvalidShape` token `fx_clearing`.
- Rows 3 to 6 missing (EUR moved into FX Clearing only): no credit side, `InvalidShape` token `fx_clearing`. FX Clearing would hold EUR 1,000.00 with no rate.
- FX Clearing Dr USD 500.00 and Cr USD 500.00: same currency on both sides, `InvalidShape` token `fx_clearing`.

## Minor-unit exponents
Amounts are integer minor units, and currencies differ in exponent (USD and EUR have 2 decimals, JPY has 0, some have 3). The rate in major units is:

r = (C_minor / 10^e_B) / (D_minor / 10^e_A) = (C_minor / D_minor) x 10^(e_A - e_B)

Synthetic example: FX Clearing Dr USD 100.00 (10000 minor, e = 2), Cr JPY 15,000 (15000 minor, e = 0). r = 15000 / 10000 x 10^(2 - 0) = **150 JPY per USD**. The raw minor-unit ratio, 1.5, is wrong by a factor of 100.

The kernel needs no exponent table, because rule 3 is about currencies, not amounts. Rate display and the plausibility check need one (ISO 4217 minor units). The rate is an exact ratio of two integers, so it is computed with exact rational or decimal arithmetic, never floating point, and rounded only for display.

## Interactions

### Reversals and corrections (ADR 0002)
A reversal cancels its target account by account, so it swaps sides. Reversing Tx 15 gives FX Clearing Dr USD 1,080.00 and Dr USD 105.00, Cr EUR 1,000.00. That is still one debit currency and one different credit currency, so the reversal passes rule 3 with no special case. Its derived rate is 1000.00 / 1185.00 = 0.843882 EUR per USD (rounded), the inverse of 1.185: the same exchange seen backwards, not a new one. Reports show a reversed group and its reversal as a cancelled pair, like the register does for other reversed pairs. The plausibility check skips reversals. A correction's replacement is a new FX group with its own derived rate, and it is checked.

### Recipe halves (ADR 0004)
Each half is one event in one entity's database, so rule 3 applies to each half alone, against that entity's own FX Clearing account. An exchange must complete inside one half. For example, Household converts EUR to USD inside its own half and its intercompany receivable is in USD. Both halves' intercompany accounts are then single-currency USD, and the expectation row is USD.

An exchange that is split across halves (Household gives EUR, Dragonfly receives USD) would leave each half's FX Clearing one-sided, so the kernel rejects it. Under decision 1 it would also need a EUR receivable facing a USD payable, which the recipe check reports as a mismatched amount. Whether that case should ever be allowed is open question 5. FX Clearing rows are never intercompany postings, so they never count toward a half's amount (ADR 0004).

### Same currency on both sides
Rejected (`InvalidShape` token `fx_clearing`). A same-currency transfer between two accounts is a plain two-line event, with no clearing. Open question 6 asks whether there is ever a reason to route one through FX Clearing.

### Reporting closed groups
- FX Clearing never appears with a balance, and `account_balances` leaves it out.
- Each FX group is listed with its date, the amount out (A), the amount in (B), and the derived rate in B per A, with a drill-down to its rows.
- Month-end reports are per currency (decision 7). A currency's balance sheet closes only together with the net amount of that currency exchanged in or out through FX Clearing during the period. That net is shown as a flow line, "net exchanged", backed by the list of groups, never as an account balance. Open question 7 asks Doug to confirm that presentation.
- The trial balance (`trial_balance`) still sums to zero per currency from `journal_postings` alone, FX Clearing rows included. It is a proof, not a report.

### Rate plausibility check
- **Where it runs:** in the application (layer 3), like the recipe check. It needs external rate data, so it is not part of the kernel `doctor`, which stays read-only, single-database and offline (ADR 0007).
- **What it does:** for every current, non-reversal FX group, it compares the derived rate with a reference rate for that currency pair on the event's effective date. It reports `implausible rate` when the difference is outside a tolerance, and `no reference rate` when it has no data for that pair and date.
- **What it does not do:** it is informational. It never writes, never blocks a commit, and never sets the write freeze. A fix is a correction made by a person.
- **Open:** the rate source, the tolerance, and whether a finding makes the check exit non-zero (open questions 3 and 4).

### Other rules
- **Settlement groups (ADR 0006):** the INV-008 zero-residual rule keeps applying to `clearing` accounts only. `fx_clearing` rows are never settlement legs.
- **Valuation (ADR 0005)** and the deferred FX translation display (GLOSSARY) are unchanged. Neither writes postings.

## Consequences
- An exchange is one ordinary balanced event. Its rate is a fact derived from the posted amounts, so there is no stored rate to drift from the amounts.
- No FX gain or loss account and no revaluation entries. Each currency's books stand alone.
- The kernel's new checks are cheap and need no market data. The external plausibility check carries the "is this amount right" question.
- Every ledger account now needs a currency at creation. A category, receivable or equity account used in two currencies becomes two accounts (open question 2).
- Resolves SPEC §13 decision 2 (FX journal model) and the native-currency part of decision 6. Retained-earnings presentation per currency stays with decision 6.
- Required tests: each accept and reject example above, the JPY exponent case, a reversal (inverse rate, shape accepted), a correction (new group), a recipe half with an exchange inside it, a recipe half whose FX Clearing is one-sided (rejected), and a fee booked to expense.

## Alternatives considered
- **Per-currency FX clearing accounts with an FX gain/loss plug.** Rejected: it needs a home currency to measure the gain in, which decision 7 rules out, and it creates a balance that means nothing without conversion.
- **Store the rate on the event and check amounts against it.** Rejected for the kernel: it duplicates what the amounts already say and adds a rounding rule. Whether to store it as a display convenience is open question 8.
- **Multi-currency ordinary accounts (today's schema).** Rejected by decision 1: a single account mixing currencies hides an exchange inside an account.

## Open questions for Doug
1. One FX Clearing account per entity, or one per currency pair? This ADR assumes one per entity (a partial unique index). Per pair would scope rule 3 per account.
2. Does "accounts are single-currency" include income and expense categories (for example, separate USD and EUR "Bank fees" leaves), receivables, payables and equity accounts? This ADR reads it as every ledger account except FX Clearing, which reverses SPEC §2.3's "categories are independent of currency". If categories are exempt, rule 1 applies to balance-sheet accounts only.
3. Rate source for the plausibility check: which provider or published reference rate, and does it run online or from a downloaded table?
4. Tolerance for "implausible", and should a finding make the check exit non-zero (like the recipe check's missing half), or stay information only?
5. Should FX across recipe halves ever be allowed (one entity gives EUR, the other receives USD)? This ADR rejects it; the exchange happens inside one half.
6. Can a same-currency transfer ever use FX Clearing? This ADR rejects it.
7. Month-end presentation: is "net exchanged" per currency, backed by the closed groups, the right way to show what FX Clearing did, given it never shows a balance?
8. Is the rate only derived, or also stored on the event for display? And what display precision (for example, 6 significant digits, or fixed decimals per pair)?
