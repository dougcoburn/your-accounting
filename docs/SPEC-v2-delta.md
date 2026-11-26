# SPEC v2 delta: kernel schema v2

**Status:** Proposed (U15, 2026-11-22). Principal approved the direction (PR 19 review) and the required fixes are applied; Doug merges.
**Applies to:** `docs/SPEC.md` (v0.2, amended 2026-11-22).
**Executable schema:** `docs/schema-v2.sql` (fresh migration 1). Smoke checks: `docs/schema-v2-smoke.sh` (synthetic data only).

This file lists what v2 changes in the v0.2 spec and why. Where this delta and `docs/SPEC.md` disagree, this delta wins. Where this delta and an ADR disagree, the ADR wins, and the delta has a bug. The SPEC text itself gets rewritten after the month-1 interaction spec freezes (U13). This delta is meant to make that rewrite mechanical.

## 1. Summary

| Area | v0.2 | v2 | Why |
|---|---|---|---|
| Storage | One SQLite database for all entities | **One database per entity**. Ids are globally unique, and foreign keys never cross databases | ADR 0004 |
| Entities | `entities.visibility` is transparent or opaque | **No visibility column.** A singleton `entity` row has a stable `party_id`, and `user_grants` holds access | ADR 0003, ADR 0004 |
| Counterparties | Opaque entities and `parties.opaque_entity_id` | **One Party concept**, including other entities, referenced by their `party_id` | ADR 0003 |
| Ledger accounts | Element plus role (contra) | `account_type` is one of asset, liability, equity, income or expense, plus `kind` and `parent_id`. Leaf-only postings. No contra type | ADR 0008 |
| Categories | `categories` table plus `allocations` | **Categories are income/expense ledger accounts.** No categories, allocations or allocation_tags tables | ADR 0008 |
| Journal | Mutable `state`/`revision` with replacement current state | **Append-only `accounting_events`** with `recorded_at` and `reverses_id`. `superseded_by` is set once, inside the reversal's commit | ADR 0002 and amendment |
| Commit | Implicit | An explicit **`commits`** row is written last. Its seal trigger validates the whole commit, and `seq` is the entity revision | ADR 0002, AT-013, AT-032 |
| Ids | Per-prefix (`je_1`) | **ULID** text keys, checked by CHECK constraints. The id source and clock are injectable | ADR 0004, ADR 0002 |
| Money | Integer or exact decimal (open) | **INTEGER minor units** plus an ISO code. STRICT tables reject REAL values | MAP 2026-11-19 (open decision 1) |
| Dates | Posting date by default, an effective-date override, reconciliation on the posting date | The **occurred date is used for everything**, including reconciliation. The posted date sits on observations only and explains in-flight items | MAP and GLOSSARY 2026-11-22 (open decision 3) |
| Transactions and movements | `canonical_transactions`, `account_movements` and `movement_observations` | **Removed.** Observations are linked to the posting on their financial account that interprets them (`posting_observations`) | ADR 0008 (postings answer "what happened to the bank or card") |
| Settlement | Mutable `state`/`revision` | Append-only **`settlement_group_states`**, with draft participants and no postings in drafts. `settlement_postings.posting_id` is UNIQUE | ADR 0006, INV-006, INV-008 |
| Voids | Not modelled | An **`observation_voids` tombstone**. The same commit must reverse every current event that interprets the observation | ADR 0002 |
| Claims | `obligations` and `obligation_settlements` | The same, plus `obligation_postings`, which links a claim to its purchase postings. A settlement cap trigger is added | ADR 0001 |
| Cross-entity | One entry spanning entities | **`cross_entity_outbox`** in the first leg's commit and an idempotent **`cross_entity_inbox`** on the counterpart | ADR 0004 |
| Evidence | Shared store with a mutable `status` | Evidence is **per entity database**. Deleting it writes an `evidence_deletions` tombstone, and status is derived | ADR 0004, INV-020 |
| Valuation | Not modelled | **`valuation_snapshots`** live outside the journal, on non-cash asset accounts only | ADR 0005 |
| Audit | `audit_operations` and `audit_changes` (before/after JSON) | **Removed.** The append-only log is the audit: commits record actor and reason, and events record lineage | ADR 0002 |
| MCP, reporting groups | In the kernel schema | **Removed from the per-entity schema.** MCP is deferred past month 1, and reporting groups span databases, so they move to app config | MAP 2026-11-22, ADR 0004 |

## 2. Changes by SPEC section

### §1 Architecture and authority
- "SQLite is the sole authoritative store" still holds, but as **one SQLite database per entity** (ADR 0004). A cross-entity recipe writes one commit per database and pairs them through the outbox. `doctor` checks the pairing.
- The evidence store is **per entity database**, not shared (ADR 0004).
- "Schema migrations … tested against prior-version fixtures": v2 is **migration 1**. No v0.2 database exists, so there is no data migration. The "upgraded database" runs in §12 start at v2.

### §2.1 Entities, parties, reporting groups
- Replace `AccountingEntity.visibility` and the transparent/opaque bullets with ADR 0003. An entity is a set of books with a stable `party_id`, and users get `user_grants`. To a viewer without access, an entity is a party.
- `Party` covers merchants, employers, people and other entities. `label` is UI wording only.
- `ReportingGroup` spans entity databases, so it lives in application config, not in the kernel schema.

### §2.2 Financial accounts
- The owner is implicit: every financial account in a database belongs to that database's entity. INV-004 holds by construction.
- New `tracking_mode` is either transparent or black_box, fixed at creation for month 1 (ADR 0005).
- Only `last4` is stored, never a full account number. This matches the U01 data guard.
- Account ownership transfer between entities (open decision 7) stays deferred.

### §2.3 Currency and amounts
- Resolved: money is a **signed integer in minor units plus a currency code**. Magnitudes (postings, observations, settlements) are positive with an explicit side or direction. Only `balance_minor` and `value_minor` columns carry a sign.

### §2.5 Evidence, observations, movements
- `CanonicalTransaction`, `AccountMovement` and `movement_observations` are removed. An observation is explained by the posting on its own financial account's ledger account, through `posting_observations`. That posting must match the observation's account, amount and currency, with inflow as debit and outflow as credit (INV-007, trigger).
- Observation natural key: `UNIQUE(feed_id, external_id)` (ADR 0007), so re-import is idempotent. `row_hash` lets the importer propose differences instead of silently replacing rows (SPEC §8).
- `occurred_date` is required. `posted_date` is optional and nullable, because the Monarch CSV has a single date (plan, open question 8).
- A later feed row for the same movement sets `corroborates_id` instead of duplicating the observation (ADR 0007).
- Voiding is an `observation_voids` tombstone (ADR 0002).

### §2.6 Internal ledger, obligations, settlement groups
- `LedgerAccount` is unchanged from the 2026-11-22 amendment, now enforced by CHECKs. `kind` must agree with `account_type`. Only `category` accounts are income/expense, and only they have parents. Accounts backed by a financial account must match its kind.
- **Settlement groups are append-only.** A group's state is its latest `settlement_group_states` row: draft (repeatable) → reconciled → dissolved, or draft → dissolved. Draft participants (observations or expected slots) carry no postings (INV-006). Postings join a group only in the commit where it reconciles (INV-008). A posting belongs to at most one group, ever (ADR 0006). Corrections reverse and repost, so the replacement posting joins the new group.
- Expected slots, such as the HSA, are draft descriptors and never committed clearing entries (ADR 0006, MAP Architect answers 2026-11-20).
- Obligations are always against a party. They link to their purchase postings (ADR 0001), and the remaining amount is derived.

### §3 Lifecycle and state machines
- No lifecycle state is stored as a mutable column. Review state (unreviewed, interpreted, reconciled, voided) is derived per observation by the `observation_review_state` view (ADR 0006, GLOSSARY "Review state"). Settlement state is the latest state row. Evidence present/deleted comes from tombstones. Statement state is derived from assertions and reconciliation (U32).
- The movement `unverified → verified` state is dropped from the schema. Corroboration, through `corroborates_id`, or statement coverage replaces it.

### §3.4 Dates (replaced)
> The **occurred date** (the transaction or purchase date) is an event's `effective_date`, and it alone assigns the event to a period. That includes statement reconciliation. Feed rows also keep the **posted date** when the feed has one. It is used only to explain the gap to a statement: the occurred-date balance at the cutoff, minus items that occurred on or before the cutoff but posted after it, plus items that posted on or before the cutoff but occurred after it, must equal the statement's ending balance. Corrections and reversals keep the effective date of the event they correct (ADR 0002). The v0.2 `effective_date_override` is removed. (Decision 2026-11-22, MAP "Statement cutoff".)

### §4 Invariants
- **INV-001** now applies per event and per currency within `journal_postings` alone (ADR 0008 and its 2026-11-26 amendment). Each database holds one entity, so there is no entity partition: postings and ledger accounts carry no `entity_id` (ADR 0004 amendment 2026-11-26). The singleton `entity` row is the database's identity.
- **INV-012 and INV-013**: "atomically replacing current state" becomes "atomically appending reversals and replacements" (ADR 0002).
- **New, from the ADR 0002 amendment:** `superseded_by` is NULL or the id of the single event whose `reverses_id` points at this event. It is set in that reversal's commit and never changed. A reversal targets a current, non-reversal event, cancels it account by account, and keeps its effective date.
- **New, from the ADR 0002 void rule:** a void commit reverses every current event that interprets the voided observation.
- Where each invariant is enforced is listed in §4 below.

### §6 Balance assertions and reconciliation arithmetic
- Assertion windows use the **occurred date**: `expected_B = asserted_A + sum(signed postings on the account with effective_date in (A.cutoff, B.cutoff])`. Comparing against a statement then adds the in-flight explanation from §3.4.
- `balance_minor` is signed in the account's normal direction: positive means an asset held or a liability owed. A card credit balance is negative.
- An assertion is never changed (INV-011). A revision is a new row with `supersedes_id`, written in an approved correction commit.

### §7 Corrections and audit (replaced)
- Replace "mutable current-state ledger plus immutable revision/audit records" with ADR 0002: **append-only commits**. A correction is one commit containing reversal events (`reverses_id`), replacement events, `superseded_by` set on each reversed event, and any group or claim rows it needs.
- Steps 1 to 6 stay, with step 1 written as `begin_transaction(expected_revision)`, so the commit's `seq` = expected_revision + 1 (AT-032). Step 6 becomes "append the commit; otherwise commit nothing".
- `audit_operations` and `audit_changes` are removed. The audit is the log itself: commit actor, reason and recorded_at, plus event lineage.
- Closed-month rule (ADR 0002): a commit is accepted only if every balance assertion with a cutoff on or after its earliest effective date, on every account it touches, recomputes unchanged. For a correction, that earliest date is **min(reversal date, replacement date)**. The reversal keeps the target's date, and the replacement may carry a corrected date (§5 Q2). **Enforced by the commit validator (U19)**, not by a trigger.

### §10 Proposed relational schema (replaced)
`docs/schema-v2.sql` is now executable DDL. It covers concrete columns, CHECKs, foreign keys, the signed-balance convention, the money representation and the journal account mapping, which §10 said were missing.

Removed: `entities.visibility`, `parties.opaque_entity_id`, `account_owner_valid`, `canonical_transactions`, `account_movements`, `movement_observations`, `event_transactions`, `scenarios` (renamed `recipes`), `transaction_tags`, `audit_operations`, `audit_changes`, `reporting_groups`, `reporting_group_members`, and the `mcp_*` tables. Also every mutable `state`/`revision`/`status` column.

Added: `entity` (singleton), `commits`, `user_grants`, `party_aliases`, `feeds`, `import_runs`, `observation_voids`, `posting_observations`, `settlement_group_states`, `settlement_draft_participants`, `obligation_postings`, `cross_entity_outbox`, `cross_entity_inbox`, `evidence_deletions`, `valuation_snapshots` and `reference_renames`.

### §11 Acceptance tests affected
- **AT-001, AT-016:** two databases, with the legs paired by the outbox and inbox. "Business owes Household" is a payable to a party in one database and a receivable in the other.
- **AT-004, AT-020:** remain draft groups. No clearing posting is ever committed. **AT-019** has to be adapted: the HSA expected slot becomes its own draft group (plan U28).
- **AT-005:** "opaque entity" becomes a party.
- **AT-009, AT-010, AT-014, AT-024:** use occurred-date windows (plan U32).
- **AT-013:** covered by the seal trigger, the deferred commit foreign key, and the smoke check "no partial state".
- **AT-022, AT-023, AT-033:** use reversal plus replacement. The smoke check covers one reversal and a recategorization.
- **AT-032:** covered by `commits.seq`.
- **AT-027, AT-028:** MCP is deferred past month 1.

### §12 Definition of done
"Empty and upgraded database" means the upgraded runs start from a v2 database (migration 1).

### §13 Open decisions
- **Resolved:** 1 (integer minor units), 3 (occurred date, with in-flight explanation), 8 (settlement exclusivity with no participation limits in month 1, ADR 0006), 9 (ADR 0002 closed-month invariant).
- **Still open:** 2, 6, 7, 10, 11, 12.

### §14 Architectural decisions
- SPEC **ADR-003** (transparent and opaque entities) is superseded by docs/adr 0003 and 0004.
- SPEC **ADR-005** (mutable current-state ledger) is superseded by docs/adr 0002.

## 3. Commit protocol (normative)
**Connection guard (PR 19 fix 1).** `PRAGMA foreign_keys` is per connection and is not stored in the database file. The kernel's `open()` MUST run `PRAGMA foreign_keys = ON` and then assert that `PRAGMA foreign_keys` returns 1. If it doesn't, `open()` MUST refuse to write: it returns a typed error and leaves the connection read-only. Without the pragma, commit-scoped rows can be inserted under a `commit_id` that never gets a `commits` row. Those rows persist unsealed and skip every seal check. The smoke script shows this on a connection with the pragma OFF.

**Write protocol (PR 19 fix 2).** `begin_transaction(expected_revision)` takes `expected_revision` from the **caller**. That is the revision the user or the planning step saw. The kernel MUST NOT recompute it inside the transaction.
1. Run `BEGIN IMMEDIATE`, which takes the write lock before any read.
2. Read `current = coalesce(max(commits.seq), 0)`. If `current != expected_revision`, fail with the AT-032 conflict and write nothing. The `commits_seal` trigger makes the same check against `commits.expected_revision` as a storage backstop. It can only catch a stale write if the kernel stores the caller's value there unchanged.
3. Insert every row of the commit. Each row carries `commit_id`, and events carry the commit's `recorded_at`. To correct an event, insert the reversal, then run `UPDATE accounting_events SET superseded_by = <reversal id> WHERE id = <target>`. That is the only UPDATE the schema accepts.
4. Insert the `commits` row **last**, with `expected_revision` set to the caller's value and `seq = expected_revision + 1` (a CHECK enforces this). The `commits_seal` trigger aborts the commit on any of these:
   - a stale revision (AT-032)
   - a recorded_at that differs from the commit's
   - an event with fewer than 2 postings (INV-002)
   - an imbalance per event and currency (INV-001)
   - a reversal that doesn't exactly cancel and flag its target (ADR 0002 amendment)
   - postings in a draft (INV-006)
   - a reconciled group with no postings, or a nonzero clearing residual (INV-008, INV-017, AT-030)
   - a void whose linked events are not reversed (ADR 0002)
   - a dissolved reconciled group whose legs are not all reversed, or whose observed legs are not reposted (§5 Q3)
5. `COMMIT`. If the commits row was never written, the deferred foreign keys fail at COMMIT and nothing persists.
6. Once sealed, a commit accepts no new rows. Open-commit triggers on all 23 commit-scoped tables enforce this, and the smoke script tries each table.

**doctor rules (U21; all must be clean, exit nonzero otherwise):**
- `PRAGMA integrity_check` returns `ok`.
- `PRAGMA foreign_key_check` returns no rows. This is a doctor rule, not just a smoke check.
- `doctor_unsealed_rows` returns no rows. It lists rows in commit-scoped tables, and rows hanging off them, whose commit has no `commits` row.
- `doctor_unbalanced_events`, `doctor_superseded_drift` and `doctor_observation_double_consumed` return no rows.
- The ADR 0004 cross-database checks (outbox/inbox pairing, evidence hashes) pass.

## 4. Where each rule is enforced

| Rule | Storage (schema-v2) | Commit validator (U19) / doctor (U21) |
|---|---|---|
| INV-001 balance per event, currency | `commits_seal` | `doctor_unbalanced_events` |
| INV-002 posting shape | CHECKs, STRICT, `commits_seal` (at least 2 postings) | — |
| INV-003 leaf category, same-type parent | `journal_postings_category_leaf`, `ledger_accounts_parent_valid` | — |
| ADR 0004 amendment: inbox target is this entity | kernel inbox apply (`EntityMismatch`); no column to check | — |
| INV-004, INV-005 one owner, one currency | singleton entity, `journal_postings_currency_valid`, `observations_currency_valid` | — |
| INV-006 drafts never post | `commits_seal` | — |
| INV-007 no double consumption | `posting_observations_valid`, observation natural key | `doctor_observation_double_consumed` |
| INV-008, INV-017 zero residual, all-or-nothing | `commits_seal` (residual), `settlement_postings` UNIQUE | Recipe completeness (U26) |
| INV-010 statement reconciliation | — | U32 |
| INV-011 assertions never changed | no UPDATE/DELETE, `supersedes_id`, valuation snapshots only on non-cash assets | — |
| INV-013 transitive correction validity | — | U19, U35 |
| INV-014 source facts never rewritten | observations and evidence append-only | — |
| INV-015 refund cap | `refund_links_valid`, `refund_links_cap` | — |
| INV-020 evidence deletion | `evidence_deletions` tombstone, no DELETE | evidence hash verify |
| ADR 0002 append-only | no-UPDATE/DELETE triggers on every journal-side table | — |
| ADR 0002 amendment `superseded_by` | `accounting_events_only_supersede`, `commits_seal` | `doctor_superseded_drift` |
| ADR 0002 closed month | — | U19 |
| ADR 0001 claim caps | `obligation_settlements_cap` | — |
| ADR 0004 outbox pairing | idempotency-key uniqueness | doctor (cross-database) |
| ADR 0005 valuation snapshots | `valuation_snapshots_noncash` | — |
| ADR 0006 exclusivity | `settlement_postings.posting_id` UNIQUE | — |
| ADR 0006 dissolving a reconciled group (§5 Q3) | `commits_seal`: all legs reversed, observed legs reposted | U19: replacement legs regrouped |
| AT-032 revision | `commits.expected_revision`, `seq = expected_revision + 1`, `commits_seal` | Kernel: `BEGIN IMMEDIATE`, compare against the caller's value (§3) |
| Foreign keys on (§3) | — | Kernel `open()` asserts the pragma; doctor: `doctor_unsealed_rows`, `PRAGMA foreign_key_check` |
| Reference names (§5 Q5) | Reference rows immutable; `reference_renames` append-only | — |

## 5. Decisions (PR 19 Principal review, 2026-11-22)
1. **Commit row last, with deferred foreign keys. Decided: keep it.** It fits `begin_transaction(expected_revision)` only when the caller supplies `expected_revision` and the kernel never re-reads it inside the transaction (§3). A header row plus a separate seal would need an UPDATE or a second row, which conflicts with ADR 0002.
2. **A reversal keeps its target's effective date. Decided: agreed.** The trigger enforces it, and the replacement may carry a corrected date. The U19 closed-month check uses min(reversal date, replacement date) (§7).
3. **Dissolving a reconciled group. Decided.** `settlement_postings.posting_id` is UNIQUE forever, so a surviving leg could never join another group. Dissolving a reconciled group therefore must reverse, in the same commit, every leg still in the group, and repost every leg that still needs reconciling. **Enforced in the seal** (`commits_seal`): every leg's event must be reversed, and every non-voided observation behind a leg must have a current interpreting posting. Putting the reposted legs into their new reconciled group is a **U19 requirement**, not a choice left to U35.
4. **Exact 1:1 match in `posting_observations`. Decided: keep it for month 1.** Splits go on the category side of one event, including cross-entity receivable legs. Relax the rule only when a real case needs two events for one bank row.
5. **Reference data. Decided: agreed.** Reference data is not commit-scoped. Display names are now **append-only in the schema**: reference rows (entity, parties, financial and ledger accounts, tags, feeds) reject every UPDATE, and a rename is a `reference_renames` row. `reference_current_names` gives the latest name, so replay and audit show what a row was called at the time. Party aliases stay in `party_aliases`.
6. **Rewrite without migration. Decided: yes (Doug).** This is the plan's open question 5. Schema v2 replaces the v0.2 kernel schema as fresh migration 1, with no data migration, because no real database exists. The "upgraded database" runs in §12 start at v2.
