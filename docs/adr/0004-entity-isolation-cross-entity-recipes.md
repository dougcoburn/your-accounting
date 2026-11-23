# ADR 0004: Entities are isolated databases; cross-entity recipes use an outbox protocol

Status: Proposed (revised 2026-11-20)

## Context
A purchase "for Dragonfly" on a Household card touches two sets of books. Doug chose a **separate database per entity**, so legs cannot share one database transaction.

## Decision
- An account's postings may only use **its owning entity's** categories and ledger accounts. Categories are per entity. There is no cross-entity transaction on an account.
- "For Dragonfly" is recipe framing. Household posts the charge to its own claim receivable (no expense category). Dragonfly gets a separate balanced transaction: its own expense category plus a payable to Household.
- **Outbox protocol**: the first leg writes, in the same database transaction, an intent row with a globally unique `recipe_instance_id` (ULID/UUID), an idempotency key, the counterpart entity and the expected amount. The counterpart leg applies idempotently. Cross-entity corrections, partial settlements and forgiveness use the same protocol.
- All ids are globally unique; foreign keys never cross databases.
- **Evidence** is stored per entity database, not shared (no cross-entity ACLs).
- A deterministic, read-only **`doctor`** command (the health check) compares outbox rows to counterparts (missing, duplicate, amount mismatch), checks per-entity/currency balance, runs SQLite integrity and foreign-key checks, verifies evidence hashes, and finds orphaned staged objects. It runs on app open, after cross-entity commits and before backups, and exits nonzero for CI.
- Restoring one entity database at a time is allowed; `doctor` flags resulting gaps.

## Consequences
Brief inconsistency windows between entities are possible and visible. Required tests: crash between legs, retry without duplicate, mismatched counterpart amount, and one database restored from an older backup.
