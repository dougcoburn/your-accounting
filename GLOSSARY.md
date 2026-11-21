# Glossary

## Language

**Observation**:
One imported record of money moving in one account, as reported by a feed. For month 1 a Monarch CSV row. Its imported category is a suggestion, not truth.
_Avoid_: transaction (ambiguous with journal entry)

**Feed**:
A source of Observations (month 1: Monarch CSV export; manual offline extraction for accounts Monarch lacks).

**Evidence**:
A document supporting an Observation or interpretation (later: saved bank statements). Not a feed in month 1.

**Accounting entity** (user-facing synonym: Division):
A set of books, stored in its own database. Month 1: Doug, Household, Dragonfly (business). Esther and Numetric are Parties. See ADR 0003.

**Visibility** (viewer-relative):
Determined by a user's access grants, not by the entity. An entity a viewer cannot access appears to them as a Party. See ADR 0003. _Avoid_: "opaque entity".

**Entity tag** (e-*):
Monarch tag Doug uses to fake entities; import maps e-* tags to Accounting entities.

**Reimbursement claim**:
A receivable (AR) created when an expense is paid on behalf of another entity; settled only by a linked reimbursement, or closed by a Write-off.

**Write-off**:
Closing an unpaid Reimbursement claim by reclassifying it (e.g. as a gift).

**Expected movement**:
A movement a recipe says must exist in another account (e.g. HSA receipt for a payroll deduction) that has not been observed. Shown as an unreviewed (draft) register item in its own uncommitted group until its counterpart exists; never parked in committed clearing (ADR 0006).

**Black-box account** (candidate name):
A destination treated like an expense (e.g. "investment contribution") for money leaving tracked books into an account not modeled in detail.

**Review state** (per line item):
Unreviewed (no recipe chosen), Interpreted (draft: recipe chosen, group not yet complete), Reconciled (settlement group complete and committed), Voided. Agreement with a statement is a separate term: Statement-matched. A transaction's overall state is derived from its lines.

**Origin**:
Observed (from a feed row) or Expected (a recipe says it should exist, not yet seen).

**Tracking mode** (of a financial account):
Transparent (balance tracked; money in and out is transfers) or Black box (money sent there is booked like an expense; nothing inside is tracked). Changed only through the mode-switch wizard.

**Contra-expense** (refund, rebate, statement credit):
A credit that reduces an expense category rather than counting as income. _Avoid_: "income" or "reward" for card-perk credits (implies taxability).

**Income**:
Reserved for inflows that are genuinely earned or received (wages, interest, business revenue). Not refunds or reimbursements.

**Match hint**:
A rule a recipe declares to find candidate participants for its settlement group (amount, date window, party, memo pattern, max participants). Hints live on the recipe, not in a global matcher.

**Commit** (correction commit):
An atomic, append-only batch of journal entries (reversals plus replacements). Valid only if every transaction and settlement group it touches is valid afterward. _Avoid_: "edit" for anything in the journal.

**Append-only journal**:
Journal entries are never updated or deleted; apparent edits in the UI are reversals plus new entries.

**Effective date vs. recorded-at**:
Effective date is when the entry counts for reporting (for corrections, the observation's date). Recorded-at is when the entry was appended to the journal.

**Minor units** (`amount_minor`):
Money stored as an integer count of the currency's smallest unit (cents for USD), always paired with a currency code. _Avoid_: floats, "dollars" as a stored value.

**Non-taxable fringe benefit** (compensation category):
Employer-paid value received but excluded from taxable wages (e.g. employer health premiums, IRC §106; employer HSA contributions, §223/W-2 box 12 W). Grossed up as compensation plus an equal expense or transfer. _Avoid_: plain "income" (implies taxable).

**Investment income** (dividends, interest, distributions):
A real event inside an investment account, journaled as a transaction with its own `recorded_at`. Part of book basis. _Avoid_: lumping it into gain/loss.

**Book basis** (non-cash asset account):
The account's balance from the journal alone. Investments: contributions + investment income - withdrawals. Real estate and other fixed assets: purchase cost + improvements (ADR 0005).

**Valuation snapshot**:
A point-in-time estimate of a non-cash asset account's market value (a brokerage statement, an appraisal or home-value estimate), recorded outside the journal (never a posting). Allowed on non-cash asset accounts such as investments, real estate and vehicles; never on cash, card or liability accounts.

**Unrealized gain/loss** (market move):
Valuation snapshot minus book basis. Derived for display only, never journaled. Only a realized event (e.g. a sale) becomes a transaction. _Avoid_: "valuation adjustment" or a plug entry.

**Transaction date vs. posting date**:
Transaction date is when the purchase happened (preferred effective date). Posting date is when the institution posted it (used to reconcile to statements and the fallback effective date).

**Party**:
Anyone outside the transparent books: merchants, employers, people (e.g. Esther, Numetric, Costco). Can hold obligations. UI may label as vendor/person/organization. _Avoid_: "opaque entity" as a separate concept; an entity a viewer cannot see simply appears as a party to that viewer.

**Health check**:
A periodic application-level scan (at least on open) that finds broken invariants across entities, such as a cross-entity recipe leg missing its counterpart, and surfaces them as flagged open items.

**Void** (of an observation):
Removing an erroneous or duplicate observation from the books. Appears as a delete to the user; recorded as an append-only tombstone commit. Releases anything that observation was linked to.

**Worklist**:
All observations not yet reconciled or voided. Open is a to-do state, not an end state.
