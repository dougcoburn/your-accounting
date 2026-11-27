# ADR 0009: Single-currency ledger accounts; one multi-currency FX Clearing account

Status: Accepted (2026-11-26, Doug; PR 33 answers 2026-11-26, Doug). One open decision remains: FX across recipe halves (below). Post-month-1 unit: it is not part of the month-1 implementation plan, and no month-1 unit depends on it. Until it is scheduled and lands, schema v2 and the kernel interface spec stay as they are.

## Context
Schema v2 puts a currency on every posting and balances every event per currency (INV-001, ADR 0008 amendment). A currency is fixed only on financial accounts (INV-004). A ledger account that is not backed by a financial account, such as a receivable, a category or a clearing account, can hold postings in any currency, and SPEC §2.3 calls categories "independent of currency".

That leaves two things open: SPEC §13 decision 2 (the journal model for an FX exchange) and decision 6 (reporting across currencies). An exchange from EUR to USD must balance EUR and USD separately. Something has to receive the EUR and give the USD, and it must not create a fictitious gain or a balance that only means something after conversion.

## Decision (Doug, 2026-11-26)
1. **Ledger accounts are single-currency.** Currency is a property of the ledger account. Every journal line must be in its account's currency. The kernel rejects any other line.
2. **Logical categories are currency-independent; ledger accounts are (category, currency).** A logical category (Dining, Bank fees) has no currency. The ledger account that takes postings is unique per (logical category, currency) pair, for example Dining (USD) and Dining (EUR). No report adds a category across currencies.
3. **One FX Clearing account per entity.** It is the one multi-currency ledger account, and the kernel treats it as exceptional.
4. **FX Clearing shape per transaction.** In each accounting event that touches FX Clearing, the FX Clearing rows have **exactly one debit currency and exactly one credit currency**. A side may have several rows in its one currency. The same currency on both sides is rejected: it is not an exchange.
5. **Derived rate.** For that event, the rate is the FX Clearing credit total divided by the FX Clearing debit total: units of the credit currency per unit of the debit currency.
6. **The rate is derived, never stored as authoritative.** The integer minor-unit amounts are the truth. The rate is computed after the minor-unit exponent adjustment and shown rounded to **7 decimal places, half-even**. A cache, if one is ever added, is derivable and non-authoritative.
7. **Effective balance is zero.** Every event leaves FX Clearing's effective balance at zero: the debit total converted at the event's own derived rate equals the credit total. Raw per-currency sums of FX Clearing are never reported as a balance. Reports show FX Clearing as a list of closed groups with their rates.
8. **INV-001 still holds.** Debits equal credits per event per currency across the whole event. The bank or cash legs balance each currency against the FX Clearing rows.
9. **Per-currency, isolated month-end reports.** No month-end revaluation and no single reporting currency. Each currency gets its own balance sheet, P&L and other reports, even within one entity, and the two currency sides are never balanced against each other.
10. **Rate plausibility check.** An external, doctor-style check flags derived rates that are implausible for the exchange date. The zero effective balance is true by construction, so this check is what catches a wrong amount. Its rate source is a port (an interface) that is expected to stay in flux. A finding is information only and never fails the check.
11. **Fees go to an expense account,** normally not through FX Clearing, so the derived rate reflects the exchange itself.
12. **The common case stays in one currency.** When a USD card pays in EUR, the expense posts in USD (plus any FX fee, in USD). The EUR amount is used only to derive an informational rate (worked example 2).

## Terms
- **FX group (exchange transaction):** one accounting event that has FX Clearing rows. It is closed when it is committed, because the kernel only accepts the shape in decision 4.
- **Debit total D, currency A:** the sum of the event's FX Clearing debit rows, all in currency A.
- **Credit total C, currency B:** the sum of the event's FX Clearing credit rows, all in currency B, and B is not A.
- **Derived rate r:** C / D, in B per A, after the minor-unit adjustment below.
- **Effective balance** of FX Clearing after an event: D x r - C, in B. It is zero for every accepted event, so the account's effective balance is always zero.

## What the kernel enforces
The kernel never stores or computes a rate. It enforces three rules, and the effective-zero invariant follows from them.

1. **Account-currency match.** A posting's currency equals its ledger account's currency, unless the account is the FX Clearing account. This replaces INV-005's narrower rule (only financial-account-backed ledger accounts) and is the same error, `CurrencyMismatch`.
2. **Ledger account currency is consistent.** Every posting ledger account except FX Clearing has a currency. FX Clearing has none, and there is at most one. A ledger account backed by a financial account has that financial account's currency. A logical category node has no currency and takes no postings.
3. **FX Clearing shape, per event.** If an event has any FX Clearing rows, then:
   - its FX Clearing debit rows are all in one currency A, and there is at least one;
   - its FX Clearing credit rows are all in one currency B, and there is at least one;
   - A is not B.

   Posting magnitudes are already positive (INV-002), so both totals are nonzero whenever both sides exist.

**Why that is enough.** With D > 0 and C > 0 the rate r = C / D is defined, and D x r - C = 0 exactly, for every accepted event. The effective balance is the sum of those per-event zeros. So the kernel does not need to test the invariant numerically and never needs exchange-rate data. The only ways to break it are a missing side (r is 0 or undefined), two currencies on one side (no single rate), or one currency on both sides (no exchange). Rule 3 rejects each of these.

**What the kernel cannot check.** Whether r is the right rate. An event that books EUR 1,000.00 against USD 11,850.00 is shaped correctly and is accepted. That is what the plausibility check is for (decision 10).

### Rejections (kernel interface spec §4.3 style)
These land in `docs/KERNEL_INTERFACE_SPEC.md` §4.3 in the same change as the schema, as §7 requires. Until then they are proposed.

| Rule | RAISE substring (proposed) | Code | If the seal raises it after U19 accepted |
|---|---|---|---|
| Posting currency is not its ledger account's (rule 1) | `posting currency must match` (existing substring; message widened to "the ledger account currency") | `CurrencyMismatch` (existing, class user) | user error (trigger, not a seal check) |
| Financial-account-backed ledger account in another currency (rule 2) | `ledger account currency must match its financial account` | `CurrencyMismatch` | user error |
| Currency missing on a posting account, or set on FX Clearing or a logical category node (rule 2) | CHECK on `ledger_accounts.currency` and `kind` | `InvalidShape` token `account_currency` (new). This row must sit above the generic "CHECK text containing `currency`" row, or it maps to `InvalidCurrency` | user error |
| A posting on a logical category node (no currency) | `leaf category` (existing substring) | `NonLeafCategory` (existing) | user error |
| FX Clearing rows with a missing side, two currencies on one side, or the same currency on both sides (rule 3) | `ADR 0009: FX Clearing rows need exactly one debit currency and one different credit currency` | `InvalidShape` token `fx_clearing` (new), refs `ADR 0009`, ids the event | `ValidatorMissed` (U19 rejects first) |
| A second FX Clearing account (decision 3) | partial UNIQUE index on `kind = 'fx_clearing'` | `InvalidShape` token `unique_other` (existing) | user error |
| A second ledger account for the same (logical category, currency) (decision 2) | partial UNIQUE index on `(parent_id, currency)` for category accounts | `InvalidShape` token `unique_other` (existing) | user error |

`InvalidCurrency` stays the format check (three uppercase letters). No new top-level code is needed.

## Schema representation (proposed delta, not yet in `docs/schema-v2.sql`)
- `ledger_accounts.currency TEXT`, same format CHECK as other currency columns. It is NULL only on the FX Clearing account and on logical category nodes.
- `ledger_accounts.kind` gains `fx_clearing`. It is distinct from `clearing`, which stays the ADR 0006 settlement clearing kind with its per-group zero residual (INV-008). `CHECK (kind <> 'fx_clearing' OR (account_type = 'asset' AND financial_account_id IS NULL AND parent_id IS NULL))`. The account type hardly matters, because the effective balance is always zero; asset matches the other clearing kind.
- Partial unique index: exactly one `fx_clearing` account per database at most (decision 3). It can be created when the first exchange is booked.
- **Categories (decision 2).** The ADR 0008 category tree stays, with currency-free nodes: these are the logical categories, and they never take postings. Each posting account for a category is a `category` ledger account with a currency whose parent is a logical leaf category, for example Dining (USD) under Dining. A partial UNIQUE index on `(parent_id, currency)` makes it unique per (category, currency). The ADR 0008 leaf-only rule then means "post only to a (category, currency) account". The exact columns are Backend's call when the unit is scheduled; the rules above are the contract.
- Trigger: a financial-account-backed ledger account's `currency` equals `financial_accounts.currency`.
- `journal_postings_currency_valid` changes from "match the financial account's currency, if any" to "match `ledger_accounts.currency`, unless the account is `fx_clearing`".
- `commits_seal` gains the rule 3 clause per event in the commit. A matching `doctor_fx_clearing_shape` view lets `doctor` re-check sealed history (`DoctorDrift` on a database the kernel sealed).
- Read models: `account_balances` leaves out `fx_clearing` rows. A new view `fx_clearing_groups(event_id, effective_date, debit_currency, debit_minor, credit_currency, credit_minor, reverses_id)` lists the groups. The rate is computed outside SQL, because it needs the minor-unit table and the rounding rule below.
- Observations may carry an optional original amount and currency from the feed (`original_amount_minor`, `original_currency`), for worked example 2. They are feed facts, not postings, and never enter INV-001.
- Smoke checks for each accept and reject path above (§7 item 3).

## Worked example 1: an exchange between two bank accounts (synthetic)
Tx 15, effective date 2026-03-15 (invented). An entity sells EUR 1,000.00 from its EUR checking account. The provider pays USD in two tranches, 1,080.00 and 105.00, and withholds a 15.00 fee from the second, so the second deposit is 90.00. The fee goes to a USD expense account, not to FX Clearing. All accounts, ids and amounts are invented.

| # | Ledger account (account currency) | Side | Amount | `amount_minor` |
|---|---|---|---|---|
| 1 | EUR checking (EUR) | Credit | EUR 1,000.00 | 100000 |
| 2 | FX Clearing (none) | Debit | EUR 1,000.00 | 100000 |
| 3 | FX Clearing (none) | Credit | USD 1,080.00 | 108000 |
| 4 | FX Clearing (none) | Credit | USD 105.00 | 10500 |
| 5 | USD checking (USD) | Debit | USD 1,080.00 | 108000 |
| 6 | USD checking (USD) | Debit | USD 90.00 | 9000 |
| 7 | Bank fees (USD) | Debit | USD 15.00 | 1500 |

**INV-001, per currency:**
- EUR: debits 100000 (row 2) = credits 100000 (row 1).
- USD: debits 108000 + 9000 + 1500 = 118500 (rows 5, 6, 7) = credits 108000 + 10500 = 118500 (rows 3, 4).

**Shape (rule 3):** FX Clearing debits are all EUR (one row), credits are all USD (two rows), and EUR is not USD. Accepted.

**Derived rate:** D = EUR 1,000.00, C = USD 1,185.00. r = 118500 / 100000 x 10^(2 - 2) = 1.185, shown as **1.1850000 USD per EUR**.

**Effective balance:** 1,000.00 x 1.185 - 1,185.00 = 0.

**Why the fee goes to expense:** had the fee been netted inside FX Clearing (row 4 as a USD 90.00 credit and no row 7), C would be 1,170.00 and r would be 1.1700000. That still balances and still has a zero effective balance, but the rate would understate the exchange by 0.015 and the fee would vanish from expenses. The kernel cannot tell the two apart; recipes book fees to expense, and the plausibility check may flag the low rate.

**Rejected variants (synthetic):**
- Row 7 posted to Bank fees (EUR) with a USD amount: `CurrencyMismatch`.
- An extra FX Clearing credit of GBP 10.00 (balanced by a GBP account): two credit currencies, `InvalidShape` token `fx_clearing`.
- Rows 3 to 6 missing (EUR moved into FX Clearing only): no credit side, `InvalidShape` token `fx_clearing`. FX Clearing would hold EUR 1,000.00 with no rate.
- FX Clearing Dr USD 500.00 and Cr USD 500.00: same currency on both sides, `InvalidShape` token `fx_clearing`. A same-currency transfer is a plain two-line event.

## Worked example 2: a USD card pays for dinner in Germany (synthetic)
The common case. Dinner costs EUR 85.00. The USD card charges USD 92.65 for it, plus a 3% foreign transaction fee of USD 2.78 (2.7795 rounded by the issuer). All amounts are invented.

**Default: no FX Clearing.** The card did the exchange, and the entity never held EUR. The event is in USD only:

| # | Ledger account (account currency) | Side | Amount | `amount_minor` |
|---|---|---|---|---|
| 1 | Dining (USD) | Debit | USD 92.65 | 9265 |
| 2 | Foreign transaction fees (USD) | Debit | USD 2.78 | 278 |
| 3 | USD card (USD) | Credit | USD 95.43 | 9543 |

USD: debits 9265 + 278 = 9543 = credits 9543. There are no EUR postings, so the dinner appears only in the USD P&L.

- **If the statement shows only the USD charge,** that is all. FX Clearing is not used, and there is no rate to derive.
- **If the statement shows both amounts** (EUR 85.00 and USD 92.65), the default still posts the USD-only event above. The EUR amount is kept on the observation as its original amount and currency, not as a posting. The application derives an informational rate from it, 9265 / 8500 x 10^(2 - 2) = **1.0900000 USD per EUR**, and the plausibility check may look at it. This rate is not an FX group and has nothing to do with FX Clearing's effective balance.
- **Option, when Doug wants the expense in EUR** (for example, tracking a trip in EUR): book the exchange through FX Clearing, and the dinner lands in the EUR P&L instead:

  | # | Ledger account | Side | Amount |
  |---|---|---|---|
  | 1 | USD card (USD) | Credit | USD 92.65 |
  | 2 | FX Clearing | Debit | USD 92.65 |
  | 3 | FX Clearing | Credit | EUR 85.00 |
  | 4 | Dining (EUR) | Debit | EUR 85.00 |
  | 5 | Foreign transaction fees (USD) | Debit | USD 2.78 |
  | 6 | USD card (USD) | Credit | USD 2.78 |

  USD: debits 9265 + 278 = 9543 = credits 9265 + 278. EUR: debits 8500 = credits 8500. Derived rate 8500 / 9265 = **0.9174312 EUR per USD**. This is a recipe choice, not the default, because it splits one card statement across two currency P&Ls.

## Minor-unit exponents and display precision
Amounts are integer minor units, and currencies differ in exponent (USD and EUR have 2 decimals, JPY has 0, some have 3). The rate in major units is:

r = (C_minor / 10^e_B) / (D_minor / 10^e_A) = (C_minor / D_minor) x 10^(e_A - e_B)

Synthetic example: FX Clearing Dr USD 100.00 (10000 minor, e = 2), Cr JPY 15,000 (15000 minor, e = 0). r = 15000 / 10000 x 10^(2 - 0) = **150.0000000 JPY per USD**. The raw minor-unit ratio, 1.5, is wrong by a factor of 100.

The kernel needs no exponent table, because rule 3 is about currencies, not amounts. Rate display and the plausibility check need one (ISO 4217 minor units). The rate is computed with exact rational or decimal arithmetic, never floating point, then rounded once, to 7 decimal places, half-even (decision 6). It is never stored as a fact.

## Interactions

### Reversals and corrections (ADR 0002)
A reversal cancels its target account by account, so it swaps sides. Reversing Tx 15 gives FX Clearing Dr USD 1,080.00 and Dr USD 105.00, Cr EUR 1,000.00. That is still one debit currency and one different credit currency, so the reversal passes rule 3 with no special case. Its derived rate is 1000.00 / 1185.00 = 0.84388185..., shown as **0.8438819 EUR per USD**: the inverse of 1.1850000, the same exchange seen backwards, not a new one. Reports show a reversed group and its reversal as a cancelled pair, like the register does for other reversed pairs. The plausibility check skips reversals. A correction's replacement is a new FX group with its own derived rate, and it is checked.

### Recipe halves (ADR 0004)
Each half is one event in one entity's database, so rule 3 applies to each half alone, against that entity's own FX Clearing account. FX Clearing rows are never intercompany postings, so they never count toward a half's amount (ADR 0004). How an exchange that crosses two entities is modelled is the open decision below.

### Reporting closed groups and month-end
- FX Clearing never appears with a balance, and `account_balances` leaves it out.
- Each FX group is listed with its date, the amount out (A), the amount in (B), and the derived rate in B per A (7 decimals), with a drill-down to its rows.
- **Month-end is per currency and isolated** (decision 9). Each currency has its own balance sheet and P&L, even within one entity, and the currencies are never balanced against each other. A currency's balance sheet closes only together with the net amount of that currency exchanged in or out through FX Clearing during the period. That **net exchanged** amount is a presentation: a flow line per currency, backed by the list of closed groups, never an account balance.
- The trial balance (`trial_balance`) still sums to zero per currency from `journal_postings` alone, FX Clearing rows included. It is a proof, not a report.

### Realized exchange gain or loss (later report)
Net exchanged is not a gain or loss. A realized exchange gain or loss only means something across several exchanges over time between the same currency pair, such as a round trip. It is a **later report computed by the application, never a kernel posting**, and it needs a matching method (for example first-in, first-out, or average) when amounts do not match exactly. That method is decided with the report.

Synthetic round trip:
- Day 1: FX Clearing Dr USD 1,185.00, Cr EUR 1,000.00. Rate 0.8438819 EUR per USD (1.1850000 USD per EUR).
- Day 30: FX Clearing Dr EUR 1,000.00, Cr USD 1,150.00. Rate 1.1500000 USD per EUR.
- The same EUR 1,000.00 went out for USD 1,185.00 and came back as USD 1,150.00. The report shows a realized loss of USD 35.00 on the round trip. Nothing is posted: each day's event already balances per currency, and the USD books already show 35.00 less cash.

### Rate plausibility check
- **Where it runs:** in the application (layer 3), like the recipe check. It needs external rate data, so it is not part of the kernel `doctor`, which stays read-only, single-database and offline (ADR 0007).
- **The rate source is a port.** The check depends on an interface, not a provider, and that interface is expected to stay in flux. Its shape: given a currency pair (A, B) and a date, answer with a reference rate and tolerance, "no reference", or "needs a person". One valid implementation is a human gut check: the application lists the group, its amounts and its rate, and a person marks it plausible or not. A file of rates or an online provider are later implementations behind the same port. This ADR picks no provider.
- **What it does:** for every current, non-reversal FX group, it asks the port about the group's pair and effective date. It reports `implausible rate` when the port says so, and `no reference rate` when the port has nothing.
- **Findings are information only.** An `implausible rate` finding becomes a warning-level to-do on the worklist, prompting a look for missing transactions in the exchange (a second tranche, a fee booked through clearing) or a call to the bank. The check never writes, never blocks a commit, never sets the write freeze, and **always exits 0**. A fix is a correction made by a person.

### Other rules
- **Settlement groups (ADR 0006):** the INV-008 zero-residual rule keeps applying to `clearing` accounts only. `fx_clearing` rows are never settlement legs.
- **Valuation (ADR 0005)** and the deferred FX translation display (GLOSSARY) are unchanged. Neither writes postings.

## OPEN DECISION for Doug: FX across recipe halves
**Case** (synthetic). Esther, once she is her own entity, wires EUR 1,000.00 from her German checking account to the Household USD checking account, which receives USD 1,185.00. Two entity databases, two currencies, one business event.

### Option A (Architect recommendation): the exchange stays inside one half
The recipe instance has a single obligation currency, and both halves' expectations are in it. By default the **sending side converts**, because the sending bank usually performs the conversion.

| Database | Debit | Credit |
|---|---|---|
| Esther | FX Clearing EUR 1,000.00 | EUR checking EUR 1,000.00 |
| Esther | AR / Household USD 1,185.00 | FX Clearing USD 1,185.00 |
| Household | USD checking USD 1,185.00 | AP / Esther USD 1,185.00 |

- Both halves expect USD 118500. The ADR 0004 recipe check works unchanged, and the rule that an exchange completes inside one half stays.
- Esther's FX group: debit EUR 1,000.00, credit USD 1,185.00, rate 1.1850000 USD per EUR. Each half balances per currency (INV-001).
- **Variant A2: the receiving side converts.** The obligation is in EUR, and Household holds the FX group: Esther records Dr AR / Household EUR 1,000.00, Cr EUR checking EUR 1,000.00. Household records Dr FX Clearing EUR 1,000.00, Cr AP / Esther EUR 1,000.00, and Dr USD checking USD 1,185.00, Cr FX Clearing USD 1,185.00. Both halves expect EUR 100000.
- **Picking the variant:** follow which bank actually performed the conversion.
- **Trade-off:** the converting side needs an intercompany account in the other currency (Esther needs AR / Household in USD). That is fine under the (category, currency) rule: it is one more single-currency ledger account.

### Option B: each half in its own currency, rate derived across the recipe instance
Esther records Dr AR / Household EUR 1,000.00, Cr EUR checking EUR 1,000.00. Household records Dr USD checking USD 1,185.00, Cr AP / Esther USD 1,185.00.
- The obligation exists in two currencies, so the recipe check cannot compare the halves' amounts without a rate.
- The zero-by-construction property is lost: no single event holds both currencies.
- The rate lives in the application layer, not in the kernel's posted amounts.
- Orphans and corrections get harder: a missing or corrected half leaves a rate with only one side.
- Not recommended.

### Option C: one FX Clearing group split across the two databases
Rejected. There is no cross-database atomicity (ADR 0004), so the zero-effective-balance invariant cannot hold per transaction. Each half's FX Clearing would be one-sided, which rule 3 rejects anyway.

### Details still open under this decision
- Who decides the variant (A or A2) for a given transfer: the recipe from the observation, or the person booking it.
- Which side carries the fee: the converting side's expense account by default, or the side whose bank charged it.

## Consequences
- An exchange is one ordinary balanced event. Its rate is derived from the posted amounts, so there is no stored rate to drift from the amounts.
- No FX gain or loss account and no revaluation entries. Each currency's books and reports stand alone. Realized exchange gain or loss is a later application report.
- The kernel's new checks are cheap and need no market data. The plausibility check, behind its port, carries the "is this amount right" question without ever failing.
- Every posting ledger account has a currency. A logical category used in two currencies gets two ledger accounts; the common card-abroad case stays in the card's currency and needs none.
- Resolves SPEC §13 decision 2 (FX journal model) and the native-currency part of decision 6. Retained-earnings presentation per currency stays with decision 6.
- Required tests: each accept and reject example above, the JPY exponent case, rate rounding to 7 decimals half-even (1.1850000, 0.8438819), a reversal (inverse rate, shape accepted), a correction (new group), a second FX Clearing account (rejected), a duplicate (category, currency) account (rejected), a posting on a logical category node (rejected), the USD-card dinner with and without FX Clearing, a recipe half whose FX Clearing is one-sided (rejected), and a fee booked to expense.

## Alternatives considered
- **Per-currency FX clearing accounts with an FX gain/loss plug.** Rejected: it needs a home currency to measure the gain in, which decision 9 rules out, and it creates a balance that means nothing without conversion.
- **Store the rate on the event and check amounts against it.** Rejected (decision 6): it duplicates what the amounts already say and adds a rounding rule to the facts.
- **Multi-currency ordinary accounts (today's schema).** Rejected by decision 1: a single account mixing currencies hides an exchange inside an account.
- **Currency-scoped logical categories** (Dining-USD as its own category). Rejected by decision 2: the category is one logical thing; only its ledger accounts are per currency.

## Open questions for Doug
1. **OPEN DECISION: FX across recipe halves.** Option A (recommended, sending side converts by default; A2 when the receiving bank converts), Option B, or Option C (rejected). See the section above, including who picks the variant and which side carries the fee.
