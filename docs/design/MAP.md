# Wayfinder map: Reconcile one real month

## Destination
Take one real month (October 2025 sample) from imported observations to a posted, balanced ledger. Every observation is either reconciled, voided (e.g. a duplicate from an extraction bug), or still open on the worklist (unreviewed or interpreted). Open is not an end state; it is work still to do. Revised 2026-11-20: dropped "open with a named reason" as a terminal state.

## Decisions so far
ADRs: docs/adr/0001-0009 (drafted 2026-11-19, revised after Eng Leads review 2026-11-20; 0008 and 0002 amendment 2026-11-22; 0009 FX Clearing 2026-11-26, post-month-1).

- Destination set (2026-11-19).
- Observation source for month 1: Monarch CSV rows; Monarch category is a suggestion, not truth. Missing accounts (e.g. HSA) supplied by Doug's offline extraction (2026-11-19).

- Baseline: Formal Spec v0.2 (+schema) adopted; add to repo as docs/SPEC.md. Grilling targets its 12 open decisions, prototype-exposed gaps, and repo drift (scenarios->recipes) (2026-11-19).

- Month-1 entities: Doug, Household, Dragonfly (Entities with books); Esther and Acme Corp are Parties (see Party decision below); Monarch e-* tags map to entities/parties on import only (2026-11-19).

- e-* tags: both cases occur. Tag matching account owner restates ownership; tag differing from account owner means cross-entity purchase -> inter-entity obligation (2026-11-19).

- Inter-entity obligations: running balance per entity pair (Monarch e-* zero-balance pattern, modeled natively); e-* tags are import hints only, not a pattern in the new software (2026-11-19). Reimbursements (e.g. Acme Corp) are linked to the specific expense(s) via AR/AP receivable; unpaid ones can later be written off as a gift (2026-11-19). One pattern for all reimbursements (Acme Corp, Dragonfly, any entity): claims link to specific purchases; partial settlement allowed, remainder stays open until paid or written off (2026-11-19).

- [SUPERSEDED 2026-11-20, see below] Expected movements (e.g. HSA receipt): post now through a clearing account; the expected movement appears in the register as an unreviewed item until a completing recipe is chosen. Completion options include manually creating the HSA account and receipt, or treating it as an outflow to a black-box category like 'investment contribution' (2026-11-19).

- Register states: origin is observed or expected; progress is unreviewed, then interpreted, then reconciled. Reconciliation is per line item, not per transaction: each line of a multi-line transaction (e.g. a paycheck) joins its own settlement group and reconciles on its own. (2026-11-19; clearing-account mechanism superseded 2026-11-20, see Architect answers.)

- Account tracking modes: an account can be transparent (balance and internals tracked) or black box (outflows booked expense-like, e.g. 'investment contribution'). HSA is transparent for month 1. Both modes are supported. Switching modes is a wizard operation that must preserve kernel invariants, by appending corrections rather than editing history (2026-11-19). ADR candidate.

- Monarch import rule (prototype adapter only; the core software has no knowledge of tags): an untagged row belongs to the owner of its account, or is a transfer between accounts of the same owner (2026-11-19).

- Card-perk statement credits (e.g. Walmart+ fee credit) are contra-expense (refund/rebate reducing the original expense category), never income; GAAP-aligned, avoids implying taxability. The credit links to the charge it offsets (2026-11-19).
- Guiding principle: follow GAAP treatment where it applies.

- Matching is recipe-owned: each recipe type declares its own match hints (amount tolerance, max participants e.g. N-sum up to 4, date window, memo/party patterns, counterparty account). Every match is made within exactly one recipe type; there is no global cross-recipe matcher. Match finding is always a suggestion: nothing auto-settles; the user confirms the suggested match or selects different participants (2026-11-19).

- Corrections are append-only: the UI may present an edit, but the journal only ever appends (reversals plus new entries) so how history changed is retained. A correction is one atomic commit; it is accepted only if, once balanced, every new and prior transaction and settlement group it touches ends in a valid state (zero residual, kernel invariants hold). Resolves settled-group correction and v0.2 open decision 9 in principle (2026-11-19). ADR candidate.
- Correcting entries are effective-dated to the date of the underlying observation, even in a closed period; reported totals for that period change, and the append-only journal records when the correction was made (effective date vs. recorded-at timestamp) (2026-11-19).

- Money: always a signed integer in the currency's minor units plus an ISO currency code (`amount_minor` + `currency`); never floating point; v0.2's exact-decimal alternative dropped. Resolves v0.2 open decision 1 for money. Non-money quantities (e.g. share counts, unit prices) are out of scope for month 1 (2026-11-19).

- Refund linkage is optional: a refund may link to the purchase(s) it refunds (preferred; uses `refund_links`, partial allowed), or post as a standalone contra-expense in a chosen category when the original is hard to find. A standalone refund can be linked later via an append-only correction. Resolves v0.2 open decision 4 (2026-11-19).

- Paycheck scope (goal): model the full gross pay and every deduction down to net pay, plus employer-paid benefits that never touch net pay (employer HSA contribution, employer-paid health premiums). Each line reconciles independently; net pay matches the bank observation; other lines settle against their destination accounts or black-box categories. Month 1 source for these lines is Doug's paystub, entered manually (2026-11-19).

- Employer-paid benefits are grossed up: posted as non-taxable fringe benefit compensation plus an equal benefit expense (e.g. health insurance) or transfer (employer HSA contribution into the transparent HSA). Kept separate from taxable income categories (2026-11-19).

- Tax withholding (federal, state, FICA) posts as tax expense each paycheck; a tax refund posts as contra-expense to the same tax category, and a balance due as additional tax expense. No prepaid-tax asset (2026-11-19).

- Retirement deferrals (pre-tax 401k, Roth 401k, HSA) are transfers into transparent accounts of an `investment` account type, not expenses; black-box 'investment contribution' remains a fallback mode (2026-11-19).

- [SUPERSEDED 2026-11-20 re: valuation plug, see below] Investment account internals (start): each investment account is a transparent financial account owned by an entity (not its own entity). Contributions/withdrawals are transfers. Monthly, the user enters the statement balance (balance assertion); the app posts the difference as an investment gain/loss entry (neither income nor expense), capturing market moves, dividends, and fees in one reconciled line (2026-11-19).

- Effective date of an observation = the occurred (transaction/purchase) date; both dates are stored when the feed has them (2026-11-19). Revised 2026-11-22: reconciliation also uses the occurred date. The posted date only explains in-flight items (occurred on one side of a statement cutoff, posted on the other); see "Statement cutoff" below. The 2026-11-19 wording ("statement reconciliation uses posting date") is superseded. Resolves v0.2 open decision 3.

- Vendors/merchants and opaque entities are one concept, Party: anyone outside the transparent books. Any party can be creditor/debtor on an obligation. 'Vendor' vs 'person/org' is UI labeling only. Revised same day: visibility is viewer-relative, not an entity property. Every set of books is an Entity; users are granted access to entities. From one viewer's perspective, entities they can't see look like parties; movements between entities a viewer can see on both sides appear as matched internal transfers. Esther joining = granting her access to Household and Dragonfly, not promoting anything. Parties remain true outsiders (merchants, employers) (2026-11-19). See ADR 0003.

- Month 1: Esther is modeled as a Party; when she joins she is converted to an Entity (with access to shared Household/Dragonfly). Conversion mechanics (re-pointing past party obligations via append-only commit) to be designed then (2026-11-19).

- Categories are per entity: each entity has its own editable category list (chart of accounts); no shared base list. Hard rule: an account's postings may only use its owning entity's categories and ledger accounts; there is no cross-entity transaction on an account. A purchase 'for Dragonfly' on a Household card is recipe framing: on Household's books it posts to Household's own claim (AR) account, not an expense category. Dragonfly's books get a separate leg (its own expense category plus payable to Household) generated by the same recipe (2026-11-19).

- Entity isolation: the kernel commits per entity; a cross-entity recipe produces separate balanced transactions in each entity's books, written by the application layer at nearly the same time, not one atomic multi-entity transaction. The application layer guarantees both legs exist and are valid. A periodic health check (at least on app open) detects a missing counterpart leg and surfaces it as an open, flagged item (expected/unreviewed) for completion (2026-11-19). ADR candidate.

- Month-1 done criteria (2026-11-20, from PM review):
  - Paycheck lines other than net pay and retirement deferrals (taxes, employer-paid benefits) may stay open with a named reason; month 1 still counts as done.
  - No statement-balance match is required for any account in month 1 (statements are not a feed yet).
  - Deferred past month 1: mode-switch wizard (month 1 picks a tracking mode per account at creation) and Esther's conversion/viewer-relative visibility (month 1 has Doug as the only user, single access grant).

- Review answers (2026-11-20, from DevOps/Principal review):
  - Forgiving a claim: the category/type (gift, capital contribution, etc.) is chosen by the user at the time of the forgiveness decision, not fixed by the system.
  - A user without access to an entity sees it only as a Party (like a vendor). They cannot write into its books; a reimbursement request goes out of band, and the other entity records it when received.
  - Corrections may land in a month already reconciled, as long as that month's month-end reconciled balances do not change; the commit validator rejects corrections that would change them.
  - Storage: a separate database per entity. Confirms ADR 0004: cross-entity legs cannot share one DB transaction; shared recipe-instance id + health check pairing is required.
  - Encryption at rest and offsite backup: not handled within the application for now.
  - Data retention / purging: future feature (v0.2 open decision 12 deferred).

- Architect review answers (2026-11-20):
  - No committed clearing for expected movements (v0.2 INV-006/008/009 stand; no amending ADR). Splits of one observation (e.g. paystub) need not commit together; each group that commits must be complete under its recipe and kernel rules. An unmatched split like the HSA either (a) is booked as an expense (black-box 'investment contribution') and commits with the paystub group, or (b) is moved out of the paystub group into its own group that stays uncommitted/draft (visible as unreviewed) until its counterpart exists.
  - Investment valuation: no gain/loss posting. Market value is recorded as point-in-time valuation snapshots (when available) and compared against the account's book basis (contributions + dividends - withdrawals). Unrealized gain/loss is computed for display, never journaled; only realized events are transactions. Replaces the plug in ADR 0005.
  - Valuation snapshots widened (2026-11-21): allowed on any non-cash asset account (investments, real estate like the house and rental, vehicles); never on cash, card, or liability accounts. A house is a fixed asset at cost basis (purchase + improvements); the mortgage is a separate liability. Depreciation for the rental is deferred.
  - FX translation (deferred, 2026-11-21): foreign-currency accounts keep exact balances in their own currency; translation into the reporting currency uses stored exchange rates per date, display-only, never journaled. Separate from valuation snapshots, which stay on non-cash assets.
  - Monarch CSV date semantics (posting vs purchase): unknown; irrelevant for month 1. Treat as the effective date.
  - Evidence store is per entity database, not shared (avoids cross-entity ACLs/authorization).

- QA review answers (2026-11-20):
  - Exploration phase: the Monarch row is the observation. Doug may cherry-pick extra Monarch rows or manually add expected other halves to build a complete golden set. The system must handle open observations gracefully so imperfect test data is still useful.
  - Golden fixture: Doug signs off the expected treatment per scenario and adds scenarios as needs appear; recipes are expected to be added to the product over time.
  - Observation lifecycle: open (unreviewed/interpreted) -> reconciled, or voided. No 'open with a reason' end state and no open-reason enum. An erroneous or duplicate observation can be deleted from the user's view; under the append-only journal this is a void/tombstone commit, not a physical delete.
  - Restore: one entity database at a time is allowed; the health check flags resulting cross-entity gaps.

- Pure double entry in the kernel (2026-11-22, ADR 0008): every ledger account has a type (asset, liability, equity, income, expense); categories are income/expense ledger accounts (per entity, tree, leaf-only postings); the v0.2 `allocations` table is removed and a purchase is simply debit expense, credit card liability. Reason: income/expense lived in a separate allocations table, so the balance could not be proved from postings alone. Contra-expense for refunds and perk credits stays (credit to the original expense account).
- Supersede flag allowed (2026-11-22, ADR 0002 amendment): facts are never updated; one `superseded_by` flag may be set only in the same commit as the reversing entry, for fast "active only" views. Balances remain the sum of postings; the register hides reversed pairs; audit sees all; `doctor` rebuilds the flag from reversal links; the journal is never compacted.
- Retirement sub-accounts (2026-11-22): pre-tax and Roth balances are separate ledger accounts even when they sit in one real account, because account holders report them separately. Resolves the remaining payroll deferral question.
- Statement cutoff (2026-11-22): the occurred date assigns a transaction to a period for everything, including reconciliation. Feed rows also keep the posted date, used only to explain the gap to the statement: occurred-date balance at the cutoff, minus items that occurred on or before it but posted after, plus items that posted on or before it but occurred after, must equal the statement ending balance.
- Settlement groups (2026-11-22): no participation limits for month 1.
- Agent approval / MCP auth (2026-11-22): deferred past month 1.
- Single-currency ledger accounts and FX Clearing (2026-11-26, Doug, ADR 0009 with PR 33 answers; post-month-1 unit, not in the month-1 plan): logical categories are currency-independent, ledger accounts are (category, currency), and every line matches its account's currency, except one multi-currency FX Clearing account per entity. Each event touching FX Clearing has exactly one debit currency and one different credit currency there; the derived rate is credit total / debit total, never stored, shown to 7 decimals half-even, so FX Clearing's effective balance is zero by construction. INV-001 stays per event per currency. Month-end reports are per currency and isolated, even within one entity; no revaluation, no single reporting currency; realized exchange gain/loss is a later application report. A USD card abroad posts in USD only. Fees go to expense. A rate plausibility check behind a rate-source port raises information-only to-dos (exit 0). FX across recipe halves (decided 2026-11-26): the converting side, as its bank statement shows, does the exchange inside its own half with one obligation currency for the recipe instance; the wire fee books on the converting side's expense account; splitting the exchange across halves is rejected.

## Not yet specified (fog)
- Party-to-entity conversion wizard (when Esther joins).
- Investment accounts, later: taxable brokerage dividends/realized gains detail (quarter map or later).
- Mode-switch wizard: exact reclassification mechanics (opening balance vs. re-pointing past postings, per-period vs. whole-history).
- v0.2 section 13 open decisions still open: 2 FX journal model (decided for post-month-1 by ADR 0009), 6 multi-currency reporting (native-currency part decided by ADR 0009; retained earnings per currency still open), 7 account ownership transfer, 8 settlement participation constraints (no limits in month 1), 10 approval/MCP auth (deferred), 11 budgets, 12 data retention. Settled: 1 money, 3 statement cutoff, 4 refunds, 5 payroll, 9 corrections.
- Balance checkpoints (deferred past month 1): rebuildable per-account balances at cutoffs to bound the cost of closed-month checks and as-of-recorded replay as corrections accumulate. Caches only, verified by `doctor`, never authoritative.
- Prototype gaps remaining: evidence strength (strong vs weak) as an input to match suggestions; editable registries UI; paired in/out transfer expected slots (brokerage).

## Out of scope
- Polished UI, reports, live bank feeds.
- Statement extractor as an observation feed. Later feature: statements saved as Evidence attached to transactions.
- Multi-month workflows. Planned follow-on map: a full quarter, where everything should reconcile unless data is missing or transactions fall near the quarter boundary.
