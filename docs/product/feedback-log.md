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

One entry per question. Fill `decision`, `rationale`, `decision_date`, `follow_up`, and `links` only after a decision is made. Do not edit earlier decisions in place; add a new entry that supersedes the old id if a decision changes.

## Entry template

```text
### <id> — <short title>

- id:
- decision_date:    YYYY-MM-DD (date the decision was made, not a transaction date)
- screen:
- question:
- decision:         (empty until decided)
- rationale:        structural only; no real amounts, merchants, accounts, or transaction dates
- follow_up:        owner and next step, if any
- links:            ADRs, issues, or later feedback ids
```

---

## Open entries

Pre-seeded from the plan. Decisions are empty.

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
