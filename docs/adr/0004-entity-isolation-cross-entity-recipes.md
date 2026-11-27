# ADR 0004: Entities are isolated databases; the application orchestrates cross-entity recipes

Status: Proposed (revised 2026-11-20; amended 2026-11-26; rewritten 2026-11-26, Doug: "Yes, rewrite it"; recipe-check and multi-currency answers 2026-11-26, Doug)

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
- **A cross-entity recipe is two halves.** Each half is one accounting event, written by the application through that database's own kernel in that database's own commit. Each event balances on its own, per currency (INV-001).
- **A shared recipe-instance ULID links the halves.** Every half carries two nullable columns on `accounting_events`, both set or both NULL:
  - the recipe-instance id (`recipe_instance_id`)
  - the other entity's `entity.id` (`counterpart_entity_id`)
- **Counterpart expectations, per currency.** Every half also has one or more rows in `event_counterpart_expectations(event_id, currency, expected_amount_minor)`, at most one per currency, each amount > 0. A row says: "the other half must carry this amount, in this currency, on its intercompany account for me."
  - The kernel requires at least one row on a recipe half (`commits_seal`) and refuses rows on a reversal or a plain event.
  - **Multi-currency halves are allowed in v1** (Doug, 2026-11-26). A half can hold postings in several currencies. Balancing stays per currency (INV-001).
- **The intercompany account.** A half's intercompany postings for the counterpart are its postings on a `receivable` or `payable` ledger account whose posting `party_id` is the counterpart's `entity.party_id`, for example AR / Dragonfly in Household or AP / Household in Dragonfly. The application resolves that party id through the registry from `counterpart_entity_id`. The kernel requires a `party_id` on every receivable or payable posting in a recipe half, and rejects a missing one with `InvalidShape` token `recipe_party` (kernel interface spec §3.9, §4.3).
- **A half's amount in a currency** is the absolute value of the net (debits minus credits) of its intercompany postings for the counterpart in that currency. Postings on other accounts (cash, an expense, an FX difference, a fee) do not count, in any currency.
  - The halves **agree** when, for every currency that either half names, each half's expectation equals the other half's amount in that currency.
  - Their nets must also be on opposite sides: one half's receivable against the other's payable.
- **An intercompany obligation that itself spans two currencies** can be one recipe instance with one expectation row per currency. Use two recipe instances, one per obligation currency, when the two obligations are created, settled or corrected on their own schedules. Then each instance can be retried, corrected or reversed alone. That is the preferred split whenever the two currencies are not one business event.
- **The halves are idempotent per database.** At most one current event per recipe-instance id per database (partial unique index `WHERE superseded_by IS NULL`).
  - A retry whose payload, expectation rows included, matches the current half returns the existing event id and seals no commit.
  - A retry with a different payload is `IdempotencyConflict`, and nothing is written.
- **Corrections** follow ADR 0002:
  - A reversal carries no recipe fields and no expectation rows.
  - A corrected replacement keeps `recipe_instance_id` and `counterpart_entity_id`, and may carry different expectation rows.
  - **The application always corrects both halves, one after the other** (Doug, 2026-11-26). The recipe check reports a mismatched amount in the brief window between the two corrections, and that window is acceptable.
  - A half that is reversed without a replacement retires its recipe-instance id in that database. A later write with that id is `IdempotencyConflict`, not a fresh half. A redone recipe uses a new recipe-instance id.
- **Partial settlements, forgiveness and repayments** are their own cross-entity recipes, each with its own recipe-instance id.

### No cross-database atomicity
**There is no atomic commit across two entity databases.** The application commits one half, then the other. A crash, a lost file, a restore from an older backup, or a refused write can leave one half without the other. That window is expected and visible. **The application-level recipe check is the detection mechanism.** Nothing else guarantees that both halves exist.

### The application-level recipe check
- **What it is.** The recipe check runs in the application (layer 3). It reads every entity database the signed-in user's registry lists. Each per-database `doctor` stays read-only and single-database (ADR 0007).
- **It needs no all-seeing login.** No owner login is required to see every database (Doug, 2026-11-26). The check works for a login that sees only one half.
- **What it reports,** for each current recipe half it can see:
  - **Counterpart not visible:** the counterpart entity (`counterpart_entity_id`) is not in this user's registry, or its registered database cannot be opened by this user. This is an information status, not a failure, and it is distinct from a missing half. The check says nothing about the other half.
  - **Missing half:** the counterpart database **is** registered and visible, and it has no current event with that recipe-instance id. This includes a half that was reversed while the other is still current. A missing half is reported only in this case.
  - **Orphaned for 7 days:** a missing half that has stayed missing for 7 days or more, counted from the visible half's `recorded_at`, is raised to a warning. The application offers to reverse the orphaned half (ADR 0002). If the recipe is redone later, it gets a new recipe-instance id, because the reversed one is retired.
  - **Duplicate:** the id is current in more than two visible databases.
  - **Mismatched counterpart:** half A's `counterpart_entity_id` is not the `entity.id` of the database that holds half B, or the reverse.
  - **Mismatched amount:** for some currency, one half's expectation differs from the other half's intercompany amount, or a currency is expected on one side and absent on the other, or the two nets are on the same side.
- **When it runs:** on app open, after every cross-entity write, and before a backup. Missing, duplicate, mismatched and orphaned findings exit non-zero for CI. "Counterpart not visible" alone does not.
- **What it does not do:** it never writes or repairs. A fix is a new recipe write, a correction, or the offered reversal, made by a person through the application.

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

| Step | Database | Event (recipe instance, counterpart, expectation rows) | Debit | Credit |
|---|---|---|---|---|
| 1. Payment, Household half | Household | `R1`, counterpart `D`, expectation USD 120000 | AR / Dragonfly 1,200.00 | Checking 1,200.00 |
| 1. Payment, Dragonfly half | Dragonfly | `R1`, counterpart `H`, expectation USD 120000 | Insurance expense 1,200.00 | AP / Household 1,200.00 |
| 2. Repayment, Dragonfly half | Dragonfly | `R2`, counterpart `H`, expectation USD 120000 | AP / Household 1,200.00 | Checking 1,200.00 |
| 2. Repayment, Household half | Household | `R2`, counterpart `D`, expectation USD 120000 | Checking 1,200.00 | AR / Dragonfly 1,200.00 |

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

Each half's intercompany amount is its AR / Dragonfly or AP / Household line, 1,200.00 USD, and each half expects that amount from the other. A crash between the two writes of step 1 leaves Household's `R1` half current with no Dragonfly `R1` half. For a login that has both databases registered, the recipe check reports a missing half, and after 7 days an orphan warning with an offer to reverse. A login that sees only Household reports "counterpart not visible" instead. Retrying the Dragonfly write with the same `R1` and the same payload completes the recipe. Retrying the Household write is a no-op that returns the existing event id.

## Consequences
- Brief inconsistency windows between entities are possible and visible.
- The kernel stays a single-database component that can be property-tested alone (ADR 0007).
- Cross-entity code lives in the application and its tests.
- Required tests:
  - a crash between halves, detected as a missing half
  - a retry with no duplicate (identical payload, no new commit)
  - a retry with a changed payload (`IdempotencyConflict`)
  - a mismatched counterpart amount, per currency
  - a two-currency half (for example USD intercompany plus an EUR fee) that agrees only on its intercompany postings
  - a half orphaned for 7 days (warning, and the offered reversal)
  - a half reversed while the other is current
  - one database restored from an older backup
  - a registry row whose file now holds a different `entity` (`RegistryMismatch`, nothing written)
  - a counterpart database missing from the registry (reported as "counterpart not visible", not as a missing half)

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
