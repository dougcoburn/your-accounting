# SPEC v2 delta: kernel schema v2

**Status:** Proposed (U15, 2026-11-22). Needs Principal approval; Doug merges.
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
- **INV-001** now applies per event and per (entity, currency) within `journal_postings` alone (ADR 0008). Because each database holds one entity, the entity partition is always that entity. `entity_id` stays on postings for explicitness and for `doctor`.
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
- Closed-month rule (ADR 0002): a commit is accepted only if every balance assertion with a cutoff on or after its earliest effective date, on every account it touches, recomputes unchanged. **Enforced by the commit validator (U19)**, not by a trigger.

### §10 Proposed relational schema (replaced)
`docs/schema-v2.sql` is now executable DDL. It covers concrete columns, CHECKs, foreign keys, the signed-balance convention, the money representation and the journal account mapping, which §10 said were missing.

Removed: `entities.visibility`, `parties.opaque_entity_id`, `account_owner_valid`, `canonical_transactions`, `account_movements`, `movement_observations`, `event_transactions`, `scenarios` (renamed `recipes`), `transaction_tags`, `audit_operations`, `audit_changes`, `reporting_groups`, `reporting_group_members`, and the `mcp_*` tables. Also every mutable `state`/`revision`/`status` column.

Added: `entity` (singleton), `commits`, `user_grants`, `party_aliases`, `feeds`, `import_runs`, `observation_voids`, `posting_observations`, `settlement_group_states`, `settlement_draft_participants`, `obligation_postings`, `cross_entity_outbox`, `cross_entity_inbox`, `evidence_deletions` and `valuation_snapshots`.

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
1. The writer reads the current revision `N = max(commits.seq)`. With `PRAGMA foreign_keys=ON`, it opens one SQLite transaction (`BEGIN IMMEDIATE`).
2. The writer inserts every row of the commit. Each carries `commit_id`, and events carry the commit's `recorded_at`. To correct an event it inserts the reversal, then `UPDATE accounting_events SET superseded_by = <reversal id> WHERE id = <target>`. This is the only UPDATE the schema accepts.
3. The writer inserts the `commits` row with `seq = N + 1` **last**. The `commits_seal` trigger checks the following and aborts on any failure:
   - the revision (AT-032)
   - the shared recorded_at
   - at least two postings per event (INV-002)
   - balance per event, entity and currency (INV-001)
   - reversals cancel their targets exactly and flag them `superseded_by` (ADR 0002 amendment)
   - drafts hold no postings (INV-006)
   - reconciled groups hold postings with zero clearing residual (INV-008, INV-017, AT-030)
   - voids reverse the events linked to them (ADR 0002)
4. The writer issues COMMIT. If the commits row was never written, the deferred foreign keys fail at COMMIT and nothing persists.
5. Once sealed, a commit accepts no new rows. Open-commit triggers on every commit-scoped table enforce this.

## 4. Where each rule is enforced

| Rule | Storage (schema-v2) | Commit validator (U19) / doctor (U21) |
|---|---|---|
| INV-001 balance per event, entity, currency | `commits_seal` | `doctor_unbalanced_events` |
| INV-002 posting shape | CHECKs, STRICT, `commits_seal` (at least 2 postings) | — |
| INV-003 leaf, same-entity category | `journal_postings_category_leaf`, `journal_postings_entity_valid`, `ledger_accounts_parent_valid` | — |
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

## 5. Open questions (for Principal review; Doug where marked)
1. **Commit row last, with deferred foreign keys.** Does this fit `begin_transaction(expected_revision)` in the existing kernel interface? If the Principal prefers a header row first plus a separate seal row, the checks move to that seal row unchanged.
2. **A reversal keeps its target's effective date.** This is my reading of "corrections are effective-dated to the underlying observation's date", and the trigger enforces it. The replacement event can carry a different date if the observation's date was itself wrong.
3. **Dissolving a reconciled group** does not, at storage level, require reversing every posting in the group. INV-013 transitive validation (U19, U35) has to decide whether a surviving leg stays ungrouped or must be reposted.
4. **Exact match in `posting_observations`.** An observation is explained by a single financial-account posting of the same amount, and splits happen on the category side. A bank row that is genuinely split across two events would need two observations or a relaxed rule.
5. **Reference data** (parties, financial and ledger accounts, tags, feeds) is not commit-scoped. Only display names can change in place, and nothing is deleted. Ledger-account re-parenting stays deferred (ADR 0008).
6. **Doug:** the plan's open question 5, "Rewrite without migration", is assumed **yes** here, per Captain's U15 assignment ("Fresh migration 1, no data migration").
