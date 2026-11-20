# ADR 0002: Append-only journal; corrections are atomic, validated commits

Status: Proposed (revised 2026-11-20). Resolves Spec v0.2 open decision 9.

## Context
Settled groups sometimes need correcting. v0.2's schema is mutate-and-audit (`state`/`revision` columns, replacement current state), which contradicts the append-only intent.

## Decision
- The journal is **append-only**, enforced by storage: `BEFORE UPDATE`/`BEFORE DELETE` triggers raise on journal, posting, settlement and observation tables.
- Every row carries `recorded_at`; reversals carry `reverses_id`. Review and settlement state are **derived from the event log**, never updated in place (schema delta required against v0.2).
- A correction is one atomic **commit**, accepted only if every new and prior transaction and settlement group it touches ends valid (zero residual, invariants hold).
- **Closed-month invariant**: a commit is accepted only if every balance assertion with a cutoff on or after its effective date, on every account it touches, recomputes to the same value. Corrections may land in an already-reconciled month as long as month-end reconciled balances do not change.
- Correcting entries are **effective-dated to the underlying observation's date**. Effective date and `recorded_at` together make "as it was at close" reconstructable (replay up to recorded_at T = state as of T).
- **Voiding** an erroneous or duplicate observation is a tombstone commit; it looks like a delete to the user and releases anything linked to it.
- Clock and id source are injectable so fixture runs and replay tests are deterministic.

## Consequences
Storage grows with corrections (fine at personal scale). The commit validator is the kernel's central responsibility. "As-of-recorded" reporting is possible but out of month-1 scope.
