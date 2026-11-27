# ADR 0001: Reimbursements are claims linked to specific purchases

Status: Proposed (revised 2026-11-20 after Eng Leads review)

## Context
Doug pays for things on behalf of other entities and parties (Dragonfly, Acme Corp, others). In Monarch he faked this with e-* tags and watched a running balance return to zero. That shows whether he is repaid in total, not which purchases were repaid.

## Decision
- Every expense paid on behalf of another entity or party creates a **reimbursement claim**: a v0.2 `obligation` (no new table) in a receivable/payable ledger account, linked to the specific purchase posting(s) (ADR 0008: no allocations table).
- A reimbursement payment settles one or more linked claims. **Partial settlement is allowed**; the remainder stays open until paid or **forgiven/written off**.
- When forgiving, the user chooses the treatment (gift, capital contribution, etc.) at decision time; the system does not fix it.
- **One pattern for everything**: Acme Corp, Dragonfly, any party or entity.
- The receivable balance equals Monarch's running e-* balance (named month-1 acceptance check), but every dollar is traceable to a purchase.
- e-* tags are import hints for the Monarch adapter only.
- Across two entities (see ADR 0004), each side holds its own claim; a partial settlement or forgiveness must be mirrored on the counterpart side as its own cross-entity recipe: two halves, written by the application and linked by a recipe-instance id (ADR 0004, rewritten 2026-11-26).

## Validator must reject
Settlement above the open claim amount; forgiving an already-settled claim; settling a voided claim.

## Consequences
Claims are linked at review time (more structure than a running balance). Enables aging, write-offs and "what's still owed" per purchase. Matching is a recipe concern (ADR 0006).
