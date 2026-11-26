# ADR 0004: Entities are isolated databases; cross-entity recipes use an outbox protocol

Status: Proposed (revised 2026-11-20; amended 2026-11-26, Doug)

## Context
A purchase "for Dragonfly" on a Household card touches two sets of books. Doug chose a **separate database per entity**, so legs cannot share one database transaction.

## Decision
See the 2026-11-26 amendment below: rows carry no `entity_id`, and the inbox apply checks its target against the singleton `entity` row.

- An account's postings may only use **its owning entity's** categories and ledger accounts. Categories are per entity. There is no cross-entity transaction on an account.
- "For Dragonfly" is recipe framing. Household posts the charge to its own claim receivable (no expense category). Dragonfly gets a separate balanced transaction: its own expense category plus a payable to Household.
- **Outbox protocol**: the first leg writes, in the same database transaction, an intent row with a globally unique `recipe_instance_id` (ULID/UUID), an idempotency key, the counterpart entity and the expected amount. The counterpart leg applies idempotently. Cross-entity corrections, partial settlements and forgiveness use the same protocol.
- All ids are globally unique; foreign keys never cross databases.
- **Evidence** is stored per entity database, not shared (no cross-entity ACLs).
- A deterministic, read-only **`doctor`** command (the health check) compares outbox rows to counterparts (missing, duplicate, amount mismatch), checks per-entity/currency balance, runs SQLite integrity and foreign-key checks, verifies evidence hashes, and finds orphaned staged objects. It runs on app open, after cross-entity commits and before backups, and exits nonzero for CI.
- Restoring one entity database at a time is allowed; `doctor` flags resulting gaps.

## Consequences
Brief inconsistency windows between entities are possible and visible. Required tests: crash between legs, retry without duplicate, mismatched counterpart amount, and one database restored from an older backup.

## Amendment (2026-11-26, Doug): no row-level entity_id
Each database holds exactly one entity, so a per-row `entity_id` can only ever hold one value. It adds a foreign key and a trigger without adding a rule. Doug decided to remove it.

- **No row-level `entity_id`.** `journal_postings`, `ledger_accounts`, `parties` and the other per-row tables carry no `entity_id` column, foreign key or trigger. Every row in a database belongs to that database's entity. The first bullet above holds by construction: an account's postings can only reach the ledger accounts and categories in its own database.
- **One identity record per database.** The existing singleton `entity` table in `docs/schema-v2.sql` is that record: `id` (ULID primary key), `name`, `party_id`, and `singleton = 1` CHECK. It is the database's self-reference for the outbox and inbox. Other databases refer to this entity only through its `party_id` (ADR 0003).
- **The outbox and inbox keep their counterpart columns.** `cross_entity_outbox.counterpart_party_id` and `cross_entity_inbox.source_party_id` are unchanged.
- **The inbox apply fails closed on the wrong target.** An inbox apply names the entity it is meant for. The kernel compares that target with this database's `entity.id` before any idempotency lookup or insert. If they differ, it returns `EntityMismatch`, writes nothing and seals no commit (`docs/KERNEL_INTERFACE_SPEC.md` §3.9, §8.15).
- **The balance check is per currency.** `doctor`'s per-entity/currency balance check above means per currency within the one entity of each database (INV-001, ADR 0008 amendment 2026-11-26).
