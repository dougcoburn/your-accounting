# ADR 0002: Append-only journal; corrections are atomic, validated commits

Status: Proposed (revised 2026-11-20; amended 2026-11-22). Resolves Spec v0.2 open decision 9.

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

## Amendment (2026-11-22): supersede flag and read model
Doug: a commit-time "superseded" flag does not violate the spirit of append-only.
- **Facts are never updated**: amounts, dates, accounts, sides, entities and links stay as written.
- One derived currency flag, `accounting_events.superseded_by` (NULL = current), may be set **only inside the same atomic commit** that writes the reversing entry, and only to that reversing entry's id. It is set once and never cleared. Storage triggers reject any other update.
- The flag exists for speed: a partial index (current, non-reversal events) serves everyday "active only" views without scanning history.
- **Balances are still the sum of all postings.** A reversal and its original net to zero, so balances never read the flag.
- The **register hides** superseded entries and their reversals; **audit** shows everything, with lineage via `reverses_id`.
- `doctor` rebuilds and verifies the flag from reversal links (`reverses_id`) and reports any drift.
- **Never compact the journal.** Speed-ups are rebuildable caches or indexes, never deletions.
- Correction of a correction works the same way: each corrected entry is superseded by its own reversal (Spec AT-033).
