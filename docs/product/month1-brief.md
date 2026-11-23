# Month-1 product brief

Product brief for the first slice of your-accounting (unit U02). Design sources of truth: [MAP](../design/MAP.md), [GLOSSARY](../../GLOSSARY.md), and ADRs under [`docs/adr/`](../adr/). This brief does not change those documents. After prototype feedback, unit U13 freezes a month-1 interaction spec.

Examples below are synthetic (for example, "Example Grocer" and `12.34`). Do not put real amounts, merchants, account identifiers, or real counterparty names in this file, the feedback log, fixtures, or commit messages.

## Destination

Take one real month of imported observations to a posted, balanced ledger. Every observation is either explained by a recipe and ends reconciled or voided, or stays visible on the worklist.

Open is work still to do, not an end state. The 2026-11-20 MAP revision dropped "open with a named reason" as a terminal state. There is no open-reason enum that marks an item done. See [Month-1 done criteria](#month-1-done-criteria) for what may still be open when month 1 is accepted.

## Global scope

**In scope for month 1**

- Import observations from a Monarch-shaped CSV (synthetic fixture in git; the real month only through `YA_MONARCH_CSV` or a gitignored `local.config.json`).
- Month-1 entities and parties as named in the [MAP](../design/MAP.md). One user, with a single access grant. Tracking mode is chosen when each account is created. The HSA is transparent.
- Register and worklist, recipe picker, settlement grouping, reconciliation, and corrections, as specified below.
- Reimbursement claims linked to specific purchases, partial settlement, and forgiveness ([ADR 0001](../adr/0001-linked-reimbursement-claims.md)).
- Cross-entity recipes that write one balanced leg per entity ([ADR 0004](../adr/0004-entity-isolation-cross-entity-recipes.md)).
- Paycheck modeling from a manually entered paystub: full gross-to-net is supported, including employer-paid benefits. Net pay and retirement deferrals must reconcile for month 1 to count as done; other paycheck lines may remain on the worklist (MAP, 2026-11-20).
- Statement endings may be typed in by hand and compared to an occurred-date balance plus in-flight items. A match is not required for month 1 to count as done (MAP, 2026-11-20).

**Out of scope for month 1**

- Polished UI, reports, live bank feeds, and the statement extractor as an observation feed.
- Multi-month workflows.
- Mode-switch wizard (deferred; month 1 picks a tracking mode at account creation). See [ADR 0005](../adr/0005-account-tracking-modes.md).
- Viewer-relative multi-user UI, and converting a party into an entity when a second person joins (deferred). Month 1 has one user. See [ADR 0003](../adr/0003-parties-and-viewer-relative-visibility.md).
- As-of-recorded reports, budget features, MCP or agent auth, FX journal model, multi-currency reporting, account ownership transfer, and data retention or purge.
- Monthly investment gain/loss postings. That idea was superseded: valuation snapshots are display-only, and unrealized gain or loss is never journaled ([ADR 0005](../adr/0005-account-tracking-modes.md)).

## Vocabulary

Use the [GLOSSARY](../../GLOSSARY.md). Short reminders used on these screens:

| Term | Meaning |
|---|---|
| Observation | One imported feed row. Its imported category is a suggestion, not truth. |
| Origin | Observed (from a feed) or expected (a recipe says it should exist). |
| Review state | Per line: unreviewed, interpreted (draft), reconciled, or voided. |
| Reconciled | The line's settlement group is complete and committed. Agreement with a statement is a separate term: statement-matched. |
| Worklist | Every observation not yet reconciled or voided. |
| Occurred date | When the transaction happened. It assigns the period, including for reconciliation. |
| Posted date | When the institution posted the row. Evidence only, used to explain a statement gap. |
| In-flight item | Occurred and posted dates fall on opposite sides of a statement cutoff. |

The Monarch CSV has a single Date column and no posted date. Import treats that Date as the occurred date. Posted date is optional and may be supplied later from statements (see [Open product questions for Doug](#open-product-questions-for-doug)).

---

## Screen: Register / worklist

**User goal.** See every line for the month, finish the ones that still need work, and know what is already committed or voided.

**What it shows**

- One row per line item (not one row per multi-line event). A paycheck appears as several lines, each with its own state.
- Origin: observed or expected.
- Review state: unreviewed, interpreted, reconciled, or voided. A transaction's overall state is derived from its lines.
- Occurred date, and posted date when present.
- Account (last-4 only in the UI), amount, direction, suggested category (from the feed, as a hint), and any entity hint from import tags.
- Worklist counts: unreviewed, interpreted, reconciled, voided, and still-open total.
- Expected lines for a recipe that is still draft (for example, an HSA receipt that has not been observed) are visible as unreviewed items in their own uncommitted group.
- Superseded journal entries and their reversals are hidden on the everyday register. An audit toggle shows them. See [Corrections](#screen-corrections).

**Key actions**

- Filter by state, origin, account, entity, or "worklist only".
- Open a line into the recipe picker or an existing settlement group.
- Void an erroneous or duplicate observation (see Corrections).
- Sort, with occurred date as the default (confirm in feedback).

**Acceptance criteria**

- Every imported fixture row is visible exactly once as an observed line, unless it has been voided (voided lines are hidden by default and available under a voided filter).
- Expected lines appear only when a chosen recipe says a counterpart must exist and it has not yet been observed or booked as a black-box expense.
- Counts of unreviewed, interpreted, reconciled, voided, and open items are shown and equal the rows in those filters.
- Reconciled and voided lines are not on the worklist. Unreviewed and interpreted lines are.
- Clearing a filter does not drop rows. State is per line: one paycheck line may be reconciled while a sibling line stays interpreted.
- The everyday register does not show superseded pairs. Turning audit on reveals them with lineage.

**Out of scope for this screen**

- Polished reporting, budgeting, or a "mark done with a reason" action that removes an item from the worklist without reconciling or voiding it.
- Editing feed rows in place. Changes go through corrections or void.

---

## Screen: Recipe picker

**User goal.** Choose how a line should be booked, preview a balanced set of postings, and start (but not silently commit) a settlement group.

**What it shows**

- The line under review, and recipe types that can apply (purchase to a category, own-account transfer, refund or perk credit as contra-expense, on-behalf purchase, reimbursement, paycheck, and similar).
- Match suggestions from the chosen recipe only. Nothing auto-settles ([ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md)).
- A debit and credit preview for the affected entity, using that entity's category tree. Only leaf categories take postings.
- For an on-behalf / cross-entity recipe: a preview of each entity's leg. The paying entity posts a claim, not the other entity's expense category. The other entity posts its own expense and a payable ([ADR 0004](../adr/0004-entity-isolation-cross-entity-recipes.md)).
- The feed category, if shown at all, is labeled as a suggestion.

**Key actions**

- Choose a recipe, or accept a suggested treatment, then fill in category, counterparties, and participants.
- Confirm the preview to move the line to interpreted (draft group). Drafts never post to the journal.
- Change or clear a recipe on a line that is not yet committed.

**Acceptance criteria**

- No suggestion commits a group. Confirming a suggestion or a manual selection is required.
- A preview that would not balance per entity and currency cannot be confirmed.
- A cross-entity recipe previews two (or more) separate balanced legs and a shared recipe-instance id, not one transaction that spans entities.
- Categories offered for a posting belong only to that posting's owning entity.
- An unmatched paycheck split such as an HSA deferral can be (a) booked as a black-box investment contribution and committed with its group, or (b) moved into its own draft group that stays unreviewed until its counterpart exists. There is no committed clearing account ([ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md)).
- Suggestion order is deterministic for the same inputs.

**Out of scope for this screen**

- A global matcher that scores across recipe types.
- Auto-settlement, including when a match is unique or amounts are exact.
- Evidence-strength ranking (listed as a later prototype gap in the MAP).
- The mode-switch wizard.

---

## Screen: Settlement grouping

**User goal.** Finish a recipe's settlement group: attach the right participants, leave a residual of zero, and commit, or leave a draft group visible until it can be completed.

**What it shows**

- The group, its recipe, participants, and residual per entity and currency.
- Suggested participants from that recipe's match hints (amount window, date window, party or memo pattern, counterpart account). The user may replace them ([ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md)).
- Reimbursement claims linked to specific purchase lines, with amount still open. Partial settlement is allowed. The remainder stays an open claim until paid or forgiven; the user chooses the forgiveness treatment (gift, capital contribution, or similar) at decision time ([ADR 0001](../adr/0001-linked-reimbursement-claims.md)).
- Refund and perk-credit links to the purchase they offset, when the user makes that link. A standalone contra-expense is allowed if the original is hard to find, and can be linked later by a correction.
- For cross-entity groups: each entity's leg and the shared recipe-instance id. A missing, duplicate, or amount-mismatched counterpart is flagged by `doctor` and appears as an expected, unreviewed worklist item ([ADR 0004](../adr/0004-entity-isolation-cross-entity-recipes.md)).
- Paycheck lines each in their own group (or as sub-lines of one paycheck, pending feedback). Retirement deferrals post as transfers into transparent investment accounts; pre-tax and Roth are separate ledger accounts even when they sit in one real account.

**Key actions**

- Confirm or replace suggested participants, then commit a complete group.
- Settle part of a claim; forgive the remainder with a chosen category.
- Link or later attach a refund to a purchase, with a link amount that cannot exceed the original.
- Leave a draft group uncommitted (interpreted / unreviewed expected slot).

**Acceptance criteria**

- A group commits only when it is complete under its recipe and the residual is zero per entity and currency. Drafts never post.
- A posting line belongs to at most one active settlement group.
- Settlement cannot exceed the open claim amount. An already-settled or voided claim cannot be settled or forgiven.
- A cross-entity commit writes the first leg and an outbox intent in one database transaction; the counterpart leg applies idempotently. `doctor` reports a missing or mismatched counterpart and exits nonzero.
- Forgiving a claim requires the user to choose a treatment category. The system does not pick one.
- A linked refund cannot exceed the remaining refundable amount of the purchase it offsets.
- After commit, every participating observed line is reconciled. Lines left in a draft group stay on the worklist.

**Out of scope for this screen**

- Auto-closing a residual to a plug or clearing account.
- Participation limits (none in month 1).
- Aging reports or collection workflows. Open claims can be listed; polished AR reporting cannot.

---

## Screen: Reconciliation

**User goal.** See whether each transparent account's occurred-date book balance, adjusted for in-flight items, agrees with a statement ending the user typed in, and explain any gap.

**What it shows**

- Per transparent financial account (cash, card, loan, and other accounts whose balances are tracked): book balance by occurred date at the cutoff, in-flight items in both directions, a statement ending if one was entered, and the difference.
- In-flight rule (MAP, 2026-11-22): occurred-date balance at the cutoff, minus items that occurred on or before the cutoff but posted after, plus items that posted on or before the cutoff but occurred after, must equal the statement ending.
- Posted date is shown as evidence for those timing items. It does not assign the period.
- The HSA and other transparent investment accounts show book basis (contributions + investment income − withdrawals) and, when a valuation snapshot has been entered, a display-only unrealized gain or loss. No gain or loss is posted ([ADR 0005](../adr/0005-account-tracking-modes.md)).
- Statement-matched is a separate label from reconciled.

**Key actions**

- Enter or clear a statement ending and cutoff for an account (typed; statements are not a feed in month 1).
- Optionally enter a posted date on a line, or a valuation snapshot on a non-cash asset account.
- Inspect in-flight items. A mismatch flags the account; it does not, in the current plan, have to block month close (confirm in feedback).

**Acceptance criteria**

- Period membership uses occurred date only.
- Given a cutoff, a statement ending, and posted dates, the in-flight identity holds on a fixture that includes items in both directions. If posted dates are absent, the screen says the statement gap cannot be explained yet and does not invent dates.
- Valuation snapshots cannot be recorded on cash, card, or liability accounts. Entering one never writes a journal posting.
- Month 1 can be accepted even when no statement ending is entered, or when a typed ending does not match (MAP: no statement-balance match is required). The screen still reports the difference when an ending is present.
- Black-box accounts do not appear as statement-reconciliation targets.

**Out of scope for this screen**

- Importing statements as a feed, or attaching statement PDFs as evidence (later).
- Using posting date as the period cutoff (superseded 2026-11-22).
- Posting a monthly investment gain or loss plug (superseded).
- Closed-period balance checkpoints (deferred caches, not authoritative).

---

## Screen: Corrections

**User goal.** Fix a committed interpretation or remove a bad observation without rewriting history, and still understand what the register used to show.

**What it shows**

- An edit in the everyday UI, backed by a reversal plus a replacement in the journal ([ADR 0002](../adr/0002-append-only-journal.md)).
- Effective date (the observation's date) and recorded-at (when the correction was appended).
- Audit view: original, reversal, and replacement, linked by `reverses_id`. Everyday register hides a superseded entry and its reversal (`superseded_by` is set only in that reversing commit).
- Void: looks like a delete; recorded as a tombstone commit that releases anything linked to the observation.
- Closed-month warning when a correction's effective date falls in a month that already has balance assertions.

**Key actions**

- Correct a committed group or posting (category, participants, links, and similar).
- Void an observation, with confirmation. Whether a reason is required is a feedback question.
- Toggle audit lineage, including correction-of-correction.

**Acceptance criteria**

- Journal rows are never updated or deleted in place, except the single set-once `superseded_by` flag written in the reversing commit.
- A correction is one atomic commit. If validation fails, no partial journal or settlement state remains.
- The commit is accepted only if every transaction and settlement group it touches is valid afterward, and every balance assertion with a cutoff on or after the effective date, on every account it touches, still recomputes to the same value.
- Correcting entries are effective-dated to the underlying observation. Recorded-at is the append time.
- Void hides the observation, takes it off the worklist, and releases claims, refund links, and group participation tied to it.
- A correction of a correction supersedes only the entry it reverses. Audit can walk the full chain.
- Balances always equal the sum of all postings, including superseded pairs that net to zero.

**Out of scope for this screen**

- As-of-recorded reports (possible from the journal, not a month-1 screen).
- Compacting or purging the journal.
- A mutate-in-place "edit" of committed facts.

---

## Month-1 done criteria

These checks are testable on the synthetic fixture in CI, and on the real month only in a local dry run that reports counts, never rows. Month 1 is done when all of the following hold.

1. **Every observation is reconciled, voided, or still on the worklist.** No observation is silently dropped. Open is a to-do state, not a named terminal reason.
2. **Worklist contents are counted and named by derived state**, not by a closed end-state enum: unreviewed, interpreted, expected and awaiting a counterpart, or flagged by `doctor` as a missing or mismatched cross-entity leg. The dry run reports counts of reconciled, voided, and open items only (no merchants, amounts, or account identifiers).
3. **Net pay and retirement deferrals are reconciled.** Other paycheck lines (tax withholdings, employer-paid benefits) may remain on the worklist; month 1 still counts as done.
4. **Unpaid reimbursement claims may remain.** A purchase that created a claim can already be reconciled; the remaining claim is an open obligation, not an unexplained observation. Partial settlement and forgiveness are available but not required to finish the month.
5. **The journal balances per entity and currency.** Debits equal credits in each partition. Book balances equal the sum of postings.
6. **No missing, duplicate, or amount-mismatched cross-entity counterpart legs.** `doctor` is clean on pairing, partition balance, integrity, and foreign keys, and exits 0 on the synthetic golden (allowed worklist items are observations still open, not broken legs).
7. **Statement match is optional.** Transparent-account statement endings, when entered, are explained by the occurred-date balance plus in-flight items, or are flagged as a difference. Month 1 does not require a statement ending, or a match, for any account.
8. **HSA is transparent.** Retirement deferrals into it are transfers, not expenses, unless the user explicitly chose the black-box fallback for an unmatched split. No investment gain or loss posting is required or expected.

## Why a line is still open

These are worklist flags for filters and counts. They are not a terminal "open reason" code list, and choosing one does not finish the item (MAP and [ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md)).

| Flag | Meaning |
|---|---|
| `unreviewed` | No recipe has been chosen. |
| `interpreted` | A recipe is chosen but the settlement group is still draft. |
| `awaiting_counterpart` | An expected movement (for example an HSA receipt) sits in its own uncommitted group. |
| `missing_counterpart_leg` | `doctor` found a cross-entity recipe instance with a missing, duplicate, or mismatched other leg. This flag must be cleared for `doctor` to pass; it is not an allowed residual at month close. |
| `paystub_line_pending` | A paycheck split line has not yet committed. Allowed at month close only for lines other than net pay and retirement deferrals. |

Voided is not a worklist flag; it is a finished end state, alongside reconciled.

## Deferred past month 1

Already decided; not reopened here:

- Mode-switch wizard, including how past postings are reclassified.
- Party-to-entity conversion and viewer-relative multi-user visibility.
- As-of-recorded reports, balance-checkpoint caches, evidence-strength ranking, editable registries UI, and paired in/out transfer expected slots for a brokerage.
- Holdings detail, taxable brokerage dividend and realized-gain breakdown, depreciation, FX translation as more than a display, live feeds, polished UI, and reports.

## Open product questions for Doug

These are still open in the plan or ADRs. This brief does not decide them. Prototype reactions belong in [feedback-log.md](feedback-log.md).

1. **Posted dates and in-flight items.** The CSV has only an occurred date. For this month, will posted dates be supplied from statements, or should reconciliation show occurred-date balances without explaining a statement gap?
2. **Statement endings.** Will statement endings be typed in by hand for this month, or skipped? If a typed ending does not match, should that only flag the account, or block anything?
3. **Chart of accounts seed.** Should each entity's category tree start from the imported Monarch categories, or from a short list defined for the product?
4. **Monarch Reviewed column.** Every source row may carry a Reviewed flag. Should the prototype ignore it and start every row as unreviewed (the current plan), or treat it as a hint?
5. **Month-1 UI end state.** Is the throwaway prototype a reference only, with a CLI as the real tool, or should the approved screens be ported to a local web UI over the Rust app (unit U37)? The MAP keeps a polished UI out of scope.
6. **Void confirmation.** Is void confirm-and-done, or should it ask for a reason? (A void reason, if any, is audit text. It is not an open-reason enum.)
7. **Worklist layout, recipe-picker flow, grouping prompts, and correction presentation.** See the pre-seeded questions in [feedback-log.md](feedback-log.md).
