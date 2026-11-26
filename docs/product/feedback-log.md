# Prototype feedback log

## Privacy rule

This log records **decisions only**. It must never contain:

- real amounts or balances
- merchant or payee names
- account numbers, last-4s from real accounts, routing numbers, or other account identifiers
- dates tied to real transactions (a decision date is fine; a transaction date is not)
- notes, memos, tags, or other feed-row text from a real import
- anything copied from a real CSV, paystub, or statement

Write rationale in structural terms only: "prefers a single worklist with state filters over separate to-do and done tabs," not a description of a real row. If a session cannot be logged without those details, leave the entry open and take the example offline.

Synthetic names such as "Example Grocer" and amounts such as `12.34` are allowed when they are clearly invented.

## Purpose

Capture Doug's prototype reactions as product decisions so unit U13 can freeze the month-1 interaction spec and fold those decisions into the MAP, glossary, and ADRs. Questions come from the month-1 implementation plan's "Prototype feedback questions" section. The screens themselves are described in [month1-brief.md](month1-brief.md).

One entry per question or review finding. Pre-seeded plan questions use `FQ-*` ids. Findings from a named prototype review use `PA-*`, `PB-*`, or `PC-*` ids. Fill `decision`, `rationale`, `decision_date`, `decided_by`, `follow_up`, and `links` only after a decision is made. Do not edit earlier decisions in place; add a new entry that supersedes the old id if a decision changes.

## Entry template

```text
### <id> — <short title>

- id:
- decision_date:    YYYY-MM-DD (date the decision was made, not a transaction date)
- decided_by:       (empty until decided)
- screen:
- question:
- decision:         (empty until decided)
- rationale:        structural only; no real amounts, merchants, accounts, or transaction dates
- follow_up:        owner and next step, if any
- links:            ADRs, issues, or later feedback ids
```

---

## Open entries

Pre-seeded from the plan (FQ-01–FQ-22), plus open follow-ups from prototype reviews. Decisions are empty until filled.

### FQ-01 — Worklist: one list or two tabs

- id: FQ-01
- decision_date:
- screen: Register / worklist
- question: One list with state filters, or separate "to do" and "done" tabs?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-02 — Worklist: which columns matter

- id: FQ-02
- decision_date:
- screen: Register / worklist
- question: Which columns matter (occurred date, posted date, account last-4, merchant, amount, suggested category, entity hint)?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-03 — Worklist: feed category as a hint

- id: FQ-03
- decision_date:
- screen: Register / worklist
- question: Should Monarch's category show as a hint, or be hidden?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-04 — Worklist: placement of expected lines

- id: FQ-04
- decision_date:
- screen: Register / worklist
- question: Should expected lines sit inline with their source, or in their own section?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-05 — Worklist: default sort

- id: FQ-05
- decision_date:
- screen: Register / worklist
- question: Is sorting by occurred date right?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-06 — Recipe picker: choose recipe first vs one-click treatment

- id: FQ-06
- decision_date:
- screen: Recipe picker
- question: Choose the recipe first and then fill in details, or pick a suggested treatment in one click?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md); [ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md)

### FQ-07 — Recipe picker: debit and credit preview

- id: FQ-07
- decision_date:
- screen: Recipe picker
- question: Should the debit/credit preview always show, or be behind a toggle?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-08 — Recipe picker: category tree depth and seed

- id: FQ-08
- decision_date:
- screen: Recipe picker
- question: How deep should the category tree go per entity, and should it start from the imported Monarch categories?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md) (open product question 3)

### FQ-09 — Recipe picker: vocabulary

- id: FQ-09
- decision_date:
- screen: Recipe picker
- question: Is the vocabulary right (recipe, interpreted, on-behalf)?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md); [GLOSSARY](../../GLOSSARY.md)

### FQ-10 — Settlement grouping: when suggestions appear

- id: FQ-10
- decision_date:
- screen: Settlement grouping
- question: Should suggestions open automatically, or only when you ask?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md); [ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md)

### FQ-11 — Settlement grouping: evidence before confirm

- id: FQ-11
- decision_date:
- screen: Settlement grouping
- question: What evidence do you need before confirming a match (dates, memo, residual)?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-12 — Settlement grouping: paycheck per-line layout

- id: FQ-12
- decision_date:
- screen: Settlement grouping
- question: How should a paycheck's per-line groups look: one card with sub-lines, or separate items?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-13 — Settlement grouping: open expected-slot reminder

- id: FQ-13
- decision_date:
- screen: Settlement grouping
- question: How should an open HSA expected slot nag you?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md); [ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md)

### FQ-14 — Reconciliation: readability of the identity

- id: FQ-14
- decision_date:
- screen: Reconciliation
- question: Is "occurred balance → in-flight items → statement ending" readable?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-15 — Reconciliation: type statement endings this month

- id: FQ-15
- decision_date:
- screen: Reconciliation
- question: Will you type statement endings by hand for this month, or skip that this month?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md) (open product questions 1 and 2)

### FQ-16 — Reconciliation: mismatch is a flag or a block

- id: FQ-16
- decision_date:
- screen: Reconciliation
- question: Should a statement mismatch block anything, or just flag it?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md) (open product question 2)

### FQ-17 — Corrections: edit presentation and hidden reversals

- id: FQ-17
- decision_date:
- screen: Corrections
- question: Should an edit look like an edit, with the reversal hidden unless audit is on?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md); [ADR 0002](../adr/0002-append-only-journal.md)

### FQ-18 — Corrections: void confirmation

- id: FQ-18
- decision_date:
- screen: Corrections
- question: Is a void confirm-and-done, or should it ask for a reason?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md) (open product question 6)

### FQ-19 — Corrections: changed-after-close markers

- id: FQ-19
- decision_date:
- screen: Corrections
- question: Should you see "changed after close" markers on closed months?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md); [ADR 0002](../adr/0002-append-only-journal.md)

### FQ-20 — Overall: daily screen

- id: FQ-20
- decision_date:
- screen: Overall
- question: Which screen would you use daily?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-21 — Overall: what is missing

- id: FQ-21
- decision_date:
- screen: Overall
- question: What is missing?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### FQ-22 — Overall: what to drop from month 1

- id: FQ-22
- decision_date:
- screen: Overall
- question: What should be dropped from month 1?
- decision:
- rationale:
- follow_up:
- links: [month1-brief.md](month1-brief.md)

### PA-06 — Map each prototype interface to the workflow it fits

- id: PA-06
- decision_date:
- decided_by:
- screen: Overall
- question: Which contexts or workflows should Prototype A (spreadsheet register), Prototype B (card triage), and Prototype C (journal-first) each serve?
- decision:
- rationale:
- follow_up: Produce this mapping after Prototype B and C feedback is also logged. Opened by PA-05.
- links: [PR #21](https://github.com/dougcoburn/your-accounting/pull/21); PA-05; PA-01; FQ-20; [month1-brief.md](month1-brief.md)

---

## Decided entries

Logged from Doug's review of Prototype A (spreadsheet register) on 2026-11-26. Source: [PR #21](https://github.com/dougcoburn/your-accounting/pull/21). These entries do not close FQ-01–FQ-22 unless a later entry says so; related open questions are cross-referenced in `links`.

### PA-01 — Navigation needs rework

- id: PA-01
- decision_date: 2026-11-26
- decided_by: Doug
- screen: Overall (Prototype A: spreadsheet register)
- question: How does navigation feel in Prototype A, and can primary workflows be reached easily?
- decision: Finding: navigation feels hard and needs rework. Requirement: primary workflows must be easy to reach. No specific navigation pattern is chosen yet; this remains an open design issue.
- rationale: From hands-on review of Prototype A's dense, keyboard-first register. The finding is about reachability of primary workflows, not about any particular row or filter.
- follow_up: Open design issue. Rework navigation so primary month-1 workflows (register/worklist, recipe and category assignment, settlement grouping, reconciliation, corrections) are easy to reach. Owner: product, with design input once each prototype interface is mapped to the workflow it fits (PA-05, PA-06).
- links: [PR #21](https://github.com/dougcoburn/your-accounting/pull/21); [month1-brief.md](month1-brief.md); FQ-20; FQ-21; PA-05; PA-06

### PA-02 — Clear an assigned recipe and category

- id: PA-02
- decision_date: 2026-11-26
- decided_by: Doug
- screen: Register / worklist (Prototype A: in-cell recipe and category picking)
- question: Can the user clear an assigned recipe or an assigned category from a transaction, or only assign them?
- decision: Requirement: the user can clear an assigned recipe and an assigned category from a transaction, not only assign them.
- rationale: Prototype A lets a recipe and a category be assigned to a transaction but does not support clearing either assignment. The month-1 brief already calls out changing or clearing a recipe on a line that is not yet committed; this entry confirms that requirement and extends the same clear action to an assigned category.
- follow_up: Include clear-recipe and clear-category in the month-1 interaction spec (U13) and in any later register or picker UI. The brief's "not yet committed" limit for clearing a recipe still stands unless a later decision says otherwise.
- links: [PR #21](https://github.com/dougcoburn/your-accounting/pull/21); [month1-brief.md](month1-brief.md) (Recipe picker: change or clear a recipe); FQ-06; FQ-21

### PA-03 — Inline add category

- id: PA-03
- decision_date: 2026-11-26
- decided_by: Doug
- screen: Recipe picker / category combobox (Prototype A: in-cell category picking)
- question: Can the user add a new category to the options list from the picker?
- decision: Requirement: the category picker supports inline "add category", and the new category is added to the options list.
- rationale: Prototype A offers a fixed category options list and has no way to add a category while picking. Month 1 needs inline creation from the picker, not only a separate chart-of-accounts editor.
- follow_up: Specify inline add-category in the interaction spec (U13). Which tree a new category joins, and whether the chart seed is imported Monarch categories or a product-defined list, remains FQ-08 (and open product question 3 in the brief).
- links: [PR #21](https://github.com/dougcoburn/your-accounting/pull/21); [month1-brief.md](month1-brief.md) (open product question 3); FQ-08

### PA-04 — Show missing sibling transactions after applying a recipe

- id: PA-04
- decision_date: 2026-11-26
- decided_by: Doug
- screen: Register / worklist; settlement grouping (Prototype A)
- question: After a recipe is applied to one transaction, does the UI show the other transactions that recipe group expects that are still missing or unmatched?
- decision: Requirement: after a recipe is applied to one transaction, show which sibling transactions the recipe group expects that are still missing or unmatched.
- rationale: In Prototype A, applying a recipe to one transaction does not surface the gaps for the other transactions that recipe group expects. The brief already treats expected lines and unmatched counterpart slots as worklist-visible; this entry requires those sibling gaps to become visible as a direct result of applying the recipe.
- follow_up: Fold into the recipe-picker and settlement-grouping interaction spec (U13). How an open expected slot should nag (FQ-13) and when match suggestions appear (FQ-10) stay open; this requirement does not choose those presentation details.
- links: [PR #21](https://github.com/dougcoburn/your-accounting/pull/21); [month1-brief.md](month1-brief.md) (expected lines, settlement grouping); [ADR 0006](../adr/0006-recipe-owned-suggestion-only-matching.md); FQ-10; FQ-13; FQ-21

### PA-05 — Prototypes A, B, and C are not a pick-one choice

- id: PA-05
- decision_date: 2026-11-26
- decided_by: Doug
- screen: Overall
- question: Should month 1 pick one of Prototypes A, B, and C as the product interface?
- decision: Prototypes A (spreadsheet register), B (card triage), and C (journal-first) are not a pick-one choice. Each interface fits specific contexts or workflows. The plan is to map each interface to the workflow it fits.
- rationale: Doug's review of Prototype A, with the same stance toward B and C: each prototype has a useful interface for particular contexts or workflows, so selecting a single prototype as the month-1 UI would drop useful fits.
- follow_up: Open item PA-06 — produce the mapping of each prototype interface to the workflow it fits, after feedback on B and C is also logged. This also informs the open navigation issue in PA-01 and FQ-20 (which screen would be used daily).
- links: [PR #21](https://github.com/dougcoburn/your-accounting/pull/21); FQ-20; FQ-21; PA-01; PA-06
