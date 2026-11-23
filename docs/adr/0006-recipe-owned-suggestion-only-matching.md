# ADR 0006: Matching is recipe-owned and always a suggestion; groups commit only when complete

Status: Proposed (revised 2026-11-20)

## Decision
- Each **recipe type declares its own match hints** (amount tolerance, max participants such as N-sum up to 4, date window, party/memo patterns, counterparty account). Every match is made within exactly one recipe type; matching is a pure suggestion function outside the kernel.
- Match finding is **always a suggestion**. Nothing auto-settles; the user confirms or selects different participants. Suggestion order is deterministic (stable tie-break) and N-sum search is bounded.
- **Settlement exclusivity**: the kernel enforces that a posting line belongs to at most one active settlement group, across all recipes.
- Reconciliation is **per line**: splits of one observation (e.g. a paystub) need not commit together, but every group that commits must be complete under its recipe and kernel rules. v0.2 INV-006/008/009 stand: no committed clearing for missing counterparts.
- An unmatched split (e.g. HSA) is either booked expense-like (black box) and commits with its group, or moved into its own group that stays draft (unreviewed) until its counterpart exists.
- Register items carry an origin (observed / expected) and a state: unreviewed, interpreted (draft), reconciled, or voided. Open items are a worklist, not an end state. "Reconciled" means settlement-group complete; agreement with a statement is called "statement-matched".

## Consequences
Predictable, auditable matching with more clicks than auto-settle. Evidence strength may later rank suggestions but never commits them.
