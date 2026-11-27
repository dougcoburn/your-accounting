# ADR 0004: Entities are isolated databases; the application orchestrates cross-entity recipes

Status: Proposed (revised 2026-11-20; amended 2026-11-26; rewritten 2026-11-26, Doug: "Yes, rewrite it")

The rewrite below is the current decision. The earlier decision text and the earlier amendment are kept under "History" so the reasoning stays traceable. Where the history and the rewrite differ, the rewrite wins.

## Context
A purchase "for Dragonfly" on a Household card touches two sets of books. Doug chose a **separate database per entity**, so legs cannot share one database transaction.

The 2026-11-20 decision paired the legs with a kernel-level outbox row in the first database and an inbox row in the second. That put cross-database orchestration inside a kernel that otherwise sees one database. Doug decided instead that the application, which can already see every database a user may see, writes both halves and checks that they agree.

## Decision (rewritten 2026-11-26, Doug)

### Databases and layers
- **One database per entity.** Each entity's books are one SQLite database. All ids are globally unique ULIDs, and foreign keys never cross databases.
- **No row-level `entity_id`** (from the 2026-11-26 amendment, still in force). Rows carry no `entity_id`. The singleton `entity` table (`id`, `name`, `party_id`, `singleton = 1` CHECK) is the database's one identity record. Another database refers to this entity only as a party whose id is this entity's `entity.party_id` (ADR 0003).
- **An account's postings may only use its owning entity's ledger accounts and categories.** That holds by construction: a database contains only its own entity's accounts. There is no cross-entity transaction on an account.
- **The kernel (layer 1) stays single-database.** It never opens, reads or writes a second entity database, and it has no outbox or inbox.
- **Layers 2 and 3 (recipes and application) can see every entity database the signed-in user may see**, through the registry below. The application, not the kernel, writes a cross-entity recipe.

### Cross-entity recipes
- **A cross-entity recipe is two halves.** Each half is one accounting event, written by the application through that database's own kernel in that database's own commit. Each event balances on its own (INV-001).
- **A shared recipe-instance ULID links the halves.** Every half carries:
  - the recipe-instance id (`accounting_events.recipe_instance_id`)
  - the other entity's `entity.id` (`counterpart_entity_id`)
  - the amount the other half is expected to carry (`counterpart_expected_amount_minor`) and its currency (`counterpart_currency`)

  These four columns are nullable and all-or-none.
- **A half's amount** is the sum of its debit postings in `counterpart_currency`. The two halves agree when each half's `counterpart_expected_amount_minor` equals the other half's amount.
- **The halves are idempotent per database.** At most one current event per recipe-instance id per database (partial unique index `WHERE superseded_by IS NULL`).
  - A retry whose payload matches the current half returns the existing event id and seals no commit.
  - A retry with a different payload is `IdempotencyConflict`, and nothing is written.
- **Corrections** follow ADR 0002:
  - A reversal does not carry a recipe-instance id.
  - A corrected replacement keeps the id and the counterpart fields of the half it replaces. The replacement may change `counterpart_expected_amount_minor` only together with a matching correction of the other half.
  - A half that is reversed without a replacement retires its recipe-instance id in that database. A later write with that id is `IdempotencyConflict`, not a fresh half.
- **Partial settlements, forgiveness and repayments** are their own cross-entity recipes, each with its own recipe-instance id.

### No cross-database atomicity
**There is no atomic commit across two entity databases.** The application commits one half, then the other. A crash, a lost file, a restore from an older backup, or a refused write can leave one half without the other. That window is expected and visible. **The application-level recipe check is the detection mechanism.** Nothing else guarantees that both halves exist.

### The application-level recipe check
- **What it is.** The recipe check runs in the application (layer 3). It reads every entity database the user's registry lists. Each per-database `doctor` stays read-only and single-database (ADR 0007).
- **What it flags,** for each recipe-instance id it can see:
  - **Missing half:** a current half exists in one database, and the counterpart database has no current event with that id. This includes a half that was reversed while the other is still current.
  - **Duplicate:** the id is current in more than two databases.
  - **Mismatched counterpart:** half A's `counterpart_entity_id` is not the `entity.id` of the database that holds half B, or the reverse.
  - **Mismatched amount:** either half's `counterpart_expected_amount_minor` or `counterpart_currency` disagrees with the other half's amount.
- **Unverifiable recipes.** A half whose counterpart database is not in this user's registry is reported as **unverifiable**, not as missing. The check is only as complete as the registry of the user who runs it.
- **When it runs:** on app open, after every cross-entity write, and before a backup. It exits non-zero for CI.
- **What it does not do:** it never writes or repairs. A fix is a new recipe write or a correction, made by a person through the application.

### The registry (layer 3)
- **Its own database.** The application keeps a registry database, separate from every entity database. It is scoped **per user login**.
- **What it maps:** each registered entity's `entity.id` and `entity.party_id` to the file location of that entity's database. A counterpart entity id, or a party id that is some entity's `party_id`, resolves through the same rows.
- **Different users see different sets.** Different logins can have different, possibly overlapping, registries. One registry never grants access to another user's databases (ADR 0003 grants still apply inside each entity database).
- **Explicit registration only.** A database enters a registry only when a user registers it. The application does not scan directories, discover files, or infer an entity from a party row.
- **Check before every write.** The application opens the target database and reads its `entity` row. It writes only if `entity.id` and `entity.party_id` both equal the registry row it resolved. On any mismatch, or a missing `entity` row, it refuses with `RegistryMismatch` and writes nothing.

### Evidence and restore
- **Evidence** is stored per entity database, not shared (no cross-entity ACLs).
- **Restore.** Restoring one entity database at a time is allowed. The recipe check flags the gaps that result.

### Worked example (synthetic)
Household pays Dragonfly's $1,200.00 insurance (120000 minor units, USD) from household checking. Dragonfly repays later. Each row is one event in one database. Recipe instances `R1` and `R2` are invented ULIDs. Household's `entity.id` is `H`, and Dragonfly's is `D`.

| Step | Database | Event (recipe instance, counterpart, expected) | Debit | Credit |
|---|---|---|---|---|
| 1. Payment, Household half | Household | `R1`, counterpart `D`, expects 120000 USD | AR / Dragonfly 1,200.00 | Checking 1,200.00 |
| 1. Payment, Dragonfly half | Dragonfly | `R1`, counterpart `H`, expects 120000 USD | Insurance expense 1,200.00 | AP / Household 1,200.00 |
| 2. Repayment, Dragonfly half | Dragonfly | `R2`, counterpart `H`, expects 120000 USD | AP / Household 1,200.00 | Checking 1,200.00 |
| 2. Repayment, Household half | Household | `R2`, counterpart `D`, expects 120000 USD | Checking 1,200.00 | AR / Dragonfly 1,200.00 |

Household records no expense in step 1: the payment becomes a claim on Dragonfly (ADR 0001).

| Account | After step 1 | After step 2 |
|---|---|---|
| Household checking | -1,200.00 | 0.00 (net) |
| Household AR / Dragonfly | +1,200.00 | 0.00 |
| Household equity effect | 0.00 | 0.00 (unchanged) |
| Dragonfly insurance expense | +1,200.00 | +1,200.00 |
| Dragonfly AP / Household | +1,200.00 owed | 0.00 |
| Dragonfly checking | 0.00 | -1,200.00 |
| Dragonfly equity effect | -1,200.00 | -1,200.00 (down by the expense) |

AR and AP net to zero. Household's equity is unchanged, and Dragonfly's equity is down by the insurance expense.

A crash between the two writes of step 1 leaves Household's `R1` half current with no Dragonfly `R1` half. The recipe check reports a missing half. Retrying the Dragonfly write with the same `R1` and the same payload completes the recipe. Retrying the Household write is a no-op that returns the existing event id.

## Consequences
- Brief inconsistency windows between entities are possible and visible.
- The kernel stays a single-database component that can be property-tested alone (ADR 0007).
- Cross-entity code lives in the application and its tests.
- Required tests:
  - a crash between halves, detected as a missing half
  - a retry with no duplicate (identical payload, no new commit)
  - a retry with a changed payload (`IdempotencyConflict`)
  - a mismatched counterpart amount
  - a half reversed while the other is current
  - one database restored from an older backup
  - a registry row whose file now holds a different `entity` (`RegistryMismatch`, nothing written)
  - a counterpart database missing from the registry (reported as unverifiable)

## History

### 2026-11-20 decision (superseded by the 2026-11-26 rewrite)
The original outbox decision, kept verbatim. The outbox protocol and the cross-database `doctor` in it are withdrawn. The bullets on per-entity categories, recipe framing, global ids, evidence, and restore carry forward into the rewrite.

- An account's postings may only use **its owning entity's** categories and ledger accounts. Categories are per entity. There is no cross-entity transaction on an account.
- "For Dragonfly" is recipe framing. Household posts the charge to its own claim receivable (no expense category). Dragonfly gets a separate balanced transaction: its own expense category plus a payable to Household.
- **Outbox protocol**: the first leg writes, in the same database transaction, an intent row with a globally unique `recipe_instance_id` (ULID/UUID), an idempotency key, the counterpart entity and the expected amount. The counterpart leg applies idempotently. Cross-entity corrections, partial settlements and forgiveness use the same protocol.
- All ids are globally unique; foreign keys never cross databases.
- **Evidence** is stored per entity database, not shared (no cross-entity ACLs).
- A deterministic, read-only **`doctor`** command (the health check) compares outbox rows to counterparts (missing, duplicate, amount mismatch), checks per-entity/currency balance, runs SQLite integrity and foreign-key checks, verifies evidence hashes, and finds orphaned staged objects. It runs on app open, after cross-entity commits and before backups, and exits nonzero for CI.
- Restoring one entity database at a time is allowed; `doctor` flags resulting gaps.


Original consequences: "Brief inconsistency windows between entities are possible and visible. Required tests: crash between legs, retry without duplicate, mismatched counterpart amount, and one database restored from an older backup."

### Amendment (2026-11-26, Doug): no row-level entity_id (partly superseded)
Still in force:
- Rows carry no `entity_id`.
- The singleton `entity` table is the database's one identity record.
- `doctor`'s balance check is per currency within the one entity (INV-001, ADR 0008 amendment).

Withdrawn by the rewrite:
- `cross_entity_outbox.counterpart_party_id` and `cross_entity_inbox.source_party_id`. Both tables are gone.
- The kernel inbox apply's `target_entity_id` check with `EntityMismatch`. The application's registry check (`RegistryMismatch`) replaces it, and the event's own `counterpart_entity_id` records the other entity.
