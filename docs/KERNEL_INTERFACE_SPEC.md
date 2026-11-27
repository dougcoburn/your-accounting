# Kernel interface specification

**Status:** Proposed (U16, 2026-11-26). Docs only. No implementation in this change.
**Storage:** `docs/schema-v2.sql` (migration 1). Smoke backstop: `docs/schema-v2-smoke.sh` (synthetic data only).
**Normative neighbors:** `docs/SPEC-v2-delta.md` (especially §3 and §5), `docs/adr/0001`–`0008`, `docs/SPEC.md`, `GLOSSARY.md`, `docs/design/MAP.md`.

**MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative. This file is the contract for the per-entity kernel that sits on schema v2. It is not a recipe catalog and it is not a storage driver.

## 0. Authority

Where two documents disagree, use this order, then the choices in §8:

1. An ADR, unless §8 records a later decision that changes it. The change may tighten a rule or widen the kernel. Widening is what §8.2 did, and it is the scope this file uses: importing already-keyed observations, recipe-idempotent event writes, and single-database `doctor` are kernel operations. ADR 0007 was amended on 2026-11-25 so it states that same wider scope, and on 2026-11-26 it removed the cross-database message protocol (ADR 0004 rewrite). §0 does not treat the wider scope as a conflict with the ADR. A later §8 decision that widens the kernel is resolved the same way: amend the ADR until the two documents name the same operations.
2. `docs/SPEC-v2-delta.md` over `docs/SPEC.md`. The delta already says: where the delta and an ADR disagree, the ADR wins and the delta has a bug.
3. `docs/schema-v2.sql` for any rule that is actually a trigger, CHECK, UNIQUE, foreign key, or STRICT table. If this spec describes a trigger the schema does not have, this spec has a bug.
4. This file over `accounting-core/docs/KERNEL_INTERFACE_SPEC-v1-SUPERSEDED.md` (renamed 2026-11-26 from `accounting-core/docs/KERNEL_INTERFACE_SPEC.md`) for the v2 kernel. That other file is the v1 `LedgerStore` contract of the `accounting-core` crate. It is not updated here, and it is not this interface. See §8.1.

The kernel scope under this order is the widened one in §1 and §8.2. Match hints, feed formats, and viewer-relative display stay outside the kernel. The earlier narrow reading of ADR 0007 (journal, invariants, and the commit validator only) is not the contract.

Synthetic fixtures only. Invented ULIDs, invented external ids, invented amounts. No real statements, no feed files, no rows from a private data directory, and nothing from an attachments folder. Identifiers in this text are revision integers and error codes, not sample books.

## 1. Boundary

One SQLite database holds one entity (ADR 0004). Foreign keys never cross databases. A counterpart entity is a `parties` row whose id is that entity's stable `party_id` (ADR 0003). There is no `visibility` column.

The kernel is the only writer of journal (events and their lines, i.e. postings), settlement, obligation, and assertion rows. Its operations are the append-only journal and commit seal, the numbered invariants, recipe-idempotent event writes (§3.9), and the `doctor` runner over its one database (§8.2). Observations, observation links, evidence and evidence links, refund links, and tags are context-owned rows in the same entity database (ADR 0011, §8.16). It never opens a second entity database. Cross-entity recipes, the registry, and the cross-database recipe check are application work (ADR 0004 rewrite, ADR 0007). It stores access grants as facts and does not interpret them. Feed rows and tag rows belong to their contexts (ADR 0011). Callers hand it a changeset. They do not get a connection and they do not send SQL. The one exception is registered in-process context participants (§3.14): they get a table-scoped handle inside an open write transaction.

| In the kernel | Outside the kernel |
|---|---|
| Append-only journal, commit seal, numbered invariants the schema enforces | Match hints and suggestion ranking (ADR 0006, ADR 0007) |
| — | Observations, `import_observations`, `void_observation`, observation links and evidence: observation import context (ADR 0011). Feed file formats and how an adapter derives an external id (ADR 0007) |
| — | Tag rows on postings (application context) and refund links (recipes context), ADR 0011. What a tag means to a person |
| One half of a cross-entity recipe as an ordinary event carrying `recipe_instance_id` and the counterpart fields; an identical retry is a no-op (§3.9) | Writing both halves, the per-user registry, the registry check before a write, the cross-database recipe check, and any UI around a gap (ADR 0004) |
| `doctor` over this one entity database | Viewer-relative presentation (ADR 0003) |

That scope is the same one §8.2 and ADR 0007 (amended 2026-11-25, clarified 2026-11-26, amended again 2026-11-26) record. Match hints, suggestion ranking, feed file formats, how an adapter derives an external id, and viewer-relative presentation stay outside. The earlier narrow kernel (journal, invariants, and the commit validator only; no import, no cross-entity writes, no `doctor`) is not the contract.

**Single writer.** Each entity database has one write transaction at a time. `begin_transaction` takes it with `BEGIN IMMEDIATE`. A second `begin_transaction` on that database returns `WriteLocked` and does not wait. The kernel does not interleave two changesets.

**Deterministic inputs.** `open` takes an injected clock and an injected id source (ADR 0002). Committed `recorded_at` values and kernel-allocated ids come from those, not from a later clock read and not from ambient randomness. Caller-supplied ULIDs are preserved when they pass the ULID check. Recipe ids are the exception: stable codes such as `card_purchase`, not ULIDs.

**Atomicity.** Durability happens only at `commit`. Any error inside the write transaction rolls it back. Nothing from that attempt remains (AT-013). The kernel does not commit a subset of the staged rows.

**Money and dates.** Magnitudes are positive integers in minor units plus an ISO 4217 code. `balance_minor` and `value_minor` are the signed columns; a card credit balance is negative (delta §2.3). `effective_date` is the occurred date and is the only date that assigns an event to a period, including reconciliation. `posted_date` lives on observations only and explains in-flight items (delta §3.4).

## 2. Revision protocol

This is the PR 19 contract. It is normative for every write.

The caller supplies `expected_revision`. That is the revision the user or the planning step saw. The kernel **MUST NOT** re-read `max(commits.seq)` to fill `expected_revision` in. A missing argument is `MissingExpectedRevision`, not a cue to look it up. A read of `max(seq)` is allowed only after the write lock, and only to compare.

1. `BEGIN IMMEDIATE`, which takes the write lock before any read of the revision. Before any changeset row is written, and before the revision compare below, the kernel reads the write freeze (§3.13, §8.13). The freeze is the latest `kernel_state` row with `state_key = 'freeze'` when that row has `active = 1`. It lives in the entity database, not in process memory. If that row is active:
   - `clear_write_freeze` continues to the revision compare. It is the only operation that may seal a further commit while the freeze is active.
   - `record_write_freeze` rolls back, writes nothing, and returns the existing freeze row's `commit_id` and `seq`. It does not return `WriteFrozen` and it does not seal a second freeze commit.
   - Every other operation rolls back and returns `WriteFrozen`. Write nothing.
2. `current = coalesce(max(commits.seq), 0)`. If `current != expected_revision`, roll back and return `StaleRevision` (AT-032). Write nothing.
3. Stage the changeset. Each commit-scoped row carries the new `commit_id`. Events carry the transaction's `recorded_at`, captured once from the injected clock at `begin_transaction` and not read again.
4. `commit` runs the commit validator (U19), then each context participant's `validate` (§3.14), then inserts the `commits` row **last**, with `expected_revision` equal to the caller's integer and `seq = expected_revision + 1`. A CHECK enforces that equality. The kernel stores the caller's integer unchanged. It does not substitute `current`. §2.2 is the exception: an identical re-import or an identical recipe retry that staged no row inserts no `commits` row and does not take the next `seq`.
5. `COMMIT`. If the `commits` row was never written, the deferred `commit_id` foreign keys fail and nothing persists.
6. A sealed commit accepts no further rows.

The `commits_seal` trigger repeats the stale-revision check against the stored `expected_revision`. It can catch a stale write only if the kernel stored the caller's value unchanged. The trigger is a backstop, not the place the kernel discovers the revision.

Empty database: `current` is 0. The caller's first revision is 0, and the first sealed `seq` is 1.

```text
caller saw 3
begin_transaction(3)
  BEGIN IMMEDIATE
  current = 3
  3 = 3, stage, then commits.expected_revision = 3, seq = 4
COMMIT

caller saw 3, and some other writer has since sealed seq 4
begin_transaction(3)
  BEGIN IMMEDIATE
  current = 4
  4 != 3 → StaleRevision, ROLLBACK, no new rows

forbidden, including as a "helpful" default
begin_transaction()
  expected_revision = max(seq)    -- the kernel filled it in
```

### 2.1 Kernel-level test U19 must add

`docs/schema-v2-smoke.sh` already shows two SQL writers that both saw revision 3: the second is rejected and leaves no row. That is the storage backstop. It is **not** the kernel test. U19 **MUST** add a kernel-level test, synthetic ids only, that fails if the kernel re-reads the revision to fill the argument:

1. Database sealed at revision 4. Call `begin_transaction(3)`. The result is `StaleRevision`. The `commits` count is unchanged. No new event, grant, or observation row exists. A kernel that does `expected_revision = max(seq)` inside the transaction would seal revision 5 and this assertion fails.
2. Database sealed at revision 4. Call `begin_transaction(4)`, stage a trivial legal changeset, `commit`. The new row has `expected_revision = 4` (the argument) and `seq = 5`.
3. The test observes the integer bound into the `commits` insert. It equals the caller's argument, not a fresh query result substituted for that argument.
4. `open` on a connection whose `PRAGMA foreign_keys` does not return 1 returns `ForeignKeysDisabled`. A following `begin_transaction` or `commit` writes nothing. Reads still work, so `doctor` can see damage.

Messages and assertions in that test contain ids and codes only.

### 2.2 Idempotent no-op: no empty commit

An identical re-import (§3.6, same `(feed_id, external_id)` and same `row_hash`) or a recipe retry whose payload equals the current event with that `recipe_instance_id` (§3.9) is a no-op when the transaction staged no other row.

The kernel has already taken `BEGIN IMMEDIATE` and compared the caller-supplied `expected_revision`. A mismatch is still `StaleRevision` and writes nothing. On a match the kernel does **not** insert a `commits` row, does **not** allocate `seq = expected_revision + 1`, and does **not** seal an empty commit. It rolls the write transaction back. Deferred foreign keys and `commits_seal` do not run, because no `commits` row is inserted. The journal is unchanged.

`commit` returns that outcome. The caller observes it as success, not as an error code. Dropping the transaction without `commit` also leaves no new row, and does not return this result.

- The result is `IdempotentNoop` with `noop = true`. `subjects` lists every already-stored subject, in request order. A subject is either `(feed_id, external_id)` or a `recipe_instance_id` with its existing event id, plus the `commit_id` and `seq` of the commit that first stored it. That pair is the original commit result. `commit` is the call that returns it.
- `coalesce(max(commits.seq), 0)` is unchanged and still equals the `expected_revision` passed into `begin_transaction`. No `commits` row from this call exists. A later `begin_transaction` with the same integer is not `StaleRevision` because of this call.
- The stored observation or event is byte-for-byte the original. Its `commit_id` is the one named in `subjects`.

A changeset that stages any new row is not this no-op. Identical subjects add no row. New subjects are staged. `commit` then seals one ordinary commit (§3.11). The result is that new commit, with `noop = false`.

```text
database sealed at seq 4
begin_transaction(4)
  identical re-import of (feed_id, external_id) first stored at seq 2
  nothing else staged
  commit → ROLLBACK, no commits insert
  commits count unchanged, max(seq) stays 4
  Ok IdempotentNoop { noop: true, subjects: [(feed_id, external_id, commit_id of seq 2, 2)] }

database sealed at seq 4
begin_transaction(4)
  one new observation, and one identical re-import
  commit seals seq 5 for the new observation only
  the identical key adds no observation row and no second commit
```

Synthetic fixtures only. Where a party or entity name is required on this path, the name is `Acme Corp`.

## 3. Design by contract

Enforcement words used below:

| Word | Meaning |
|---|---|
| kernel | `open` or `begin_transaction`, before any changeset row is written |
| storage | trigger, CHECK, UNIQUE, foreign key, or STRICT table in `docs/schema-v2.sql` |
| U19 | commit validator, before the `commits` row is inserted |
| U21 | `doctor`, read-only, after the fact |

U19 **MUST** reject every condition `commits_seal` raises, and return the user-correctable code, before the seal runs. If the seal then raises one of those conditions, the kernel rolls back and returns `ValidatorMissed` (a bugcheck) carrying that underlying code. Storage rules that are not in the seal (shape, append-only, leaf, currency, caps) stay user-correctable or import errors when they fire. §4 lists which is which.

`commit` is the only operation that makes rows durable. Postconditions of the staging operations hold inside the open transaction and become durable together at `commit`, or not at all.

### 3.1 `open(path, clock, ids)`

| | |
|---|---|
| Preconditions | `path` is one entity database. `clock` and `ids` are the injected sources for this process. SQLite is at least 3.37 (STRICT tables). |
| Postconditions | The kernel has executed `PRAGMA foreign_keys = ON` and then read `PRAGMA foreign_keys`. When the read is 1, the handle accepts writes. When the read is not 1, the handle is read-only: every later write returns `ForeignKeysDisabled` and stages nothing. Reads, including `doctor`, still run. |
| Invariants | PR 19 fix 1. The pragma is per connection and is not stored in the file. Without it, rows can persist under a `commit_id` that has no `commits` row and skip the seal. |
| Typed error | `ForeignKeysDisabled` (bugcheck). `StorageFault` if the file cannot be opened. |
| Enforced by | kernel `open`. Backstop: U21 `doctor_unsealed_rows` and `PRAGMA foreign_key_check`. |

`begin_transaction` and `commit` re-read the pragma. If it is no longer 1, they roll back and return `ForeignKeysDisabled`.

### 3.2 `begin_transaction(expected_revision)`

| | |
|---|---|
| Preconditions | Handle is writable (`open` asserted the pragma). `expected_revision` was supplied by the caller and is an integer ≥ 0. No write transaction is already open on this database. |
| Postconditions | `BEGIN IMMEDIATE` is held. The freeze check in §2 ran before the revision compare. `current = coalesce(max(seq), 0)` equals `expected_revision`. The transaction's `recorded_at` is fixed from the injected clock. The caller's integer is the value that `commit` will store, except the §2.2 no-op, which stores no new revision. On `StaleRevision`, `WriteLocked`, or `WriteFrozen` the database is unchanged. |
| Invariants | AT-032. ADR 0002 (one atomic commit). Single-writer rule in §1. The freeze row in §3.13. |
| Typed error | `MissingExpectedRevision`, `StaleRevision`, `WriteLocked`, `WriteFrozen`, `ForeignKeysDisabled`. |
| Enforced by | kernel, in the order in §2. Storage backstop: `commits.expected_revision`, CHECK `seq = expected_revision + 1`, `commits_seal` AT-032 message. A seal AT-032 during `commit` is `ValidatorMissed`, not a second chance to pick a revision. |

### 3.3 `append_event` (event plus postings)

Stages one non-reversal event and its postings (lines), and nothing else (ADR 0011). Tags, observation links, refund links, and evidence links are staged by their owning context and reference the event or line by id (§8.16). A reversal is `correct`, not this operation. An event that is one half of a cross-entity recipe is an ordinary `append_event` that also carries the four recipe fields; §3.9 adds its idempotency rule.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. Caller-supplied ids are 26-character Crockford base32 ULIDs, or the kernel allocates them from the injected source. `recipe_id` + `recipe_version` already exist (AT-012). At least two postings. Each posting has one ledger account, one currency, an explicit `debit` or `credit`, and `amount_minor > 0`. Per currency inside the event, sum of debits equals sum of credits. Postings carry no `entity_id`: every ledger account in the database is the entity's (ADR 0004 amendment 2026-11-26). An income or expense posting uses a leaf category. A posting on a financial-account-backed ledger account uses that account's currency. `recorded_at` is the transaction's fixed timestamp. `effective_date` is an occurred date (`YYYY-MM-DD`). `superseded_by` is null. The event's commit is still open. *Moved (ADR 0011):* the observation-link match rule (INV-007, delta §5 Q4) and the not-voided rule belong to the observation import context. |
| Postconditions | The event and its postings are staged in this transaction. Context rows that reference them are staged by their participants' `stage` in this same transaction (§3.14, ADR 0011). They are invisible to other connections until `commit`. Caller-supplied ids are unchanged. Balances, once committed, are the sum of all postings and do not read `superseded_by` (ADR 0002). |
| Invariants | INV-001, INV-002, INV-003, INV-004, INV-005. (INV-007 moved to observation import; INV-015 moved to recipes; ADR 0011.) ADR 0008 (no contra type; categories are income/expense accounts). ADR 0002 (append-only; an event is inserted current). |
| Typed error | `Unbalanced`, `InsufficientPostings`, `NonLeafCategory`, `CategoryParentInvalid`, `LedgerKindMismatch`, `CurrencyMismatch`, `UnknownRecipe`, `UnknownTarget`, `InvalidId`, `DuplicateId`, `InvalidMoney`, `InvalidCurrency`, `InvalidDate`, `CommitSealed`, `EventInsertedSuperseded`, `AppendOnlyViolation`. |
| Enforced by | U19 for INV-001 and INV-002 before the seal; the seal raises the same two and is then a `ValidatorMissed` bugcheck. Storage for leaf, currency, open commit, and "inserted current". U21 `doctor_unbalanced_events` after the fact (`doctor_observation_double_consumed` is U22, observation import, ADR 0010/0011); a row there on a database this kernel sealed is `DoctorDrift`. |

Category splits of one observation are other postings on the same event. They are not a second interpreting event for the same bank row (delta §5 Q4).

### 3.4 `correct` (reversal plus replacement)

One transaction. The kernel stages, in this order: the reversal event and its cancelling postings, then `UPDATE accounting_events SET superseded_by = <reversal id> WHERE id = <target>`, then the replacement event and its postings, then any group or claim rows that replacement needs. That UPDATE is the only update the schema accepts.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. The target is a current event: `reverses_id` is null and `superseded_by` is null. The target is not itself a reversal. The reversal's `effective_date` equals the target's `effective_date`. The reversal's postings exactly cancel the target, account by account and currency by currency (opposite side, same accounts, same amounts). The replacement is a normal `append_event` and **MAY** carry a different `effective_date`. Correcting a correction reverses the replacement, never the earlier reversal (AT-033). `superseded_by` is set once, inside this still-open transaction, to the reversal's id, and is not cleared or pointed at any other event. Every group, obligation, refund link, and assertion the change touches is valid afterward (INV-013); links are re-linked to the replacement lines by each participant's `stage`, and validated by its `validate`, in this same transaction (§3.14, ADR 0011). Closed-month: every existing balance assertion with a cutoff on or after the earliest `effective_date` in this transaction, on every financial account the transaction posts to, recomputes to the same `balance_minor`. Earliest date is `min(reversal effective_date, replacement effective_date)` (delta §7, §5 Q2). |
| Postconditions | After `commit`, the target's `superseded_by` is the reversal id. The register (`register_events`) shows the replacement and hides the target and the reversal. Audit still returns every event, with lineage through `reverses_id`. Account balances equal the plain sum of every posting. A second correction of the new replacement repeats the same shape. |
| Invariants | ADR 0002 and its amendment. AT-032, AT-033. INV-001, INV-011, INV-012, INV-013. |
| Typed error | `ReversalTargetInvalid`, `ReversalDoesNotCancel`, `SupersedeInvalid`, `SupersedeMissing`, `EventInsertedSuperseded`, `ClosedMonthViolation`, `TransitiveInvalid`, plus any `append_event` error on the replacement. |
| Enforced by | Storage: `accounting_events_reversal_valid` (current non-reversal, same effective date), `accounting_events_only_supersede` (set once, to the reversing event, inside its open commit), `accounting_events_insert_current`, and the seal (exact cancel, flag present in this commit). U19 **MUST** reject those seal conditions first; a seal raise is `ValidatorMissed`. U19 alone rejects `ClosedMonthViolation` and `TransitiveInvalid` (no trigger). U21 `doctor_superseded_drift` rebuilds the flag from `reverses_id`; a row there is `DoctorDrift`. |

`superseded_by` **MUST NOT** be set in a later transaction. The smoke script covers a second set, a clear, a set after seal, a reversal that never flags its target, a reversal of a reversal, and a reversal whose date differs from the target.

### 3.5 `void_observation(observation_id, reason)`

**Owner (ADR 0011, amended Doug 2026-11-27):** this is an observation import context operation. It tombstones the observation and ends that observation's live links. It does **not** reverse or correct kernel events, and it does not call `correct` (§3.4). The kernel event and its lines stay as they are. A line's origin (`posting_origin`) is `observed` while any live observation link points at it, and `expected` otherwise. A link is live while its observation has no `observation_voids` row. `posting_observations` rows stay (append-only); the void ends them by the tombstone, not by deleting them. There is no review state `voided`. A voided observation leaves `observation_review_state`.

A void writes only import-context rows. `observation_voids.commit_id` needs a `commits` row, so the transaction seals one. That row's `kind` is the kernel's non-reversal kind for a context-only write: `import` (context rows, no events). There is no commit kind `void`.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. The observation exists and has no `observation_voids` row. `reason` is `duplicate`, `extraction_error`, or `other`. The observation row itself is not deleted. No kernel event is staged. |
| Postconditions | After `commit`, an `observation_voids` row exists and the commit `kind` is `import`. The linked kernel event and its lines are unchanged: no new event, no reversal, `superseded_by` unchanged. The voided observation's links are not live. A line that lost its only live link is `expected`. A line that still has another live link, such as a duplicate observation, stays `observed`. A later `posting_observations` insert for this observation fails. Source facts stay in `observations` (INV-014). |
| Invariants | INV-014. Origin is derived from live links. |
| Typed error | `ObservationAlreadyVoided`, `InvalidShape` token `void_reason`, `UnknownTarget`, `InvalidId`. |
| Enforced by | Storage `posting_observations_not_voided` and UNIQUE `observation_voids.observation_id`. There is no DELETE path; DELETE on `observations` is `AppendOnlyViolation`. The seal does not require a reversal. |

### 3.6 `import_observations`

**Owner moved (ADR 0011):** this is an observation import context operation, not a kernel operation. It writes through its `ContextTx` inside a kernel transaction (§3.14). The rules below stand, enforced by that context. The §2.2 no-op applies when neither the kernel nor any participant staged a row.

The kernel stores facts. The adapter has already turned a feed file into rows. Natural key is `(feed_id, external_id)` (ADR 0007).

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. Each row has a known feed, a non-empty `external_id`, a financial account in this database, `occurred_date`, optional `posted_date`, direction `inflow` or `outflow`, `amount_minor > 0`, the account's currency, a 64-character hex `row_hash`, and `payload_json` that is valid JSON. `corroborates_id`, when set, is an observation on the same financial account and is not this row. `payload_json` stays in this database. |
| Postconditions | Same natural key and same `row_hash`: no new observation row, success, idempotent. The stored row is not replaced. When the transaction staged nothing else, `commit` is the §2.2 no-op: the caller receives `IdempotentNoop` naming this observation's original `commit_id` and `seq`, and the kernel seals no new commit. Same natural key and a different `row_hash`: the transaction rolls back with `ImportConflict` and the stored observation is unchanged (INV-014). The importer proposes a difference outside the kernel; it does not silently replace the row. A new key is staged. A later feed row for the same movement uses `corroborates_id` instead of a second independent observation. Review state of a brand-new observation is `unreviewed` unless this same transaction also interprets it. |
| Invariants | ADR 0007. INV-005, INV-014. SPEC §8 (re-import proposes differences). §2.2. |
| Typed error | `ImportConflict` (class import), `CorroborationMismatch` (class import), `CurrencyMismatch` (class user, §4.1), `InvalidId`, `InvalidMoney`, `InvalidDate`, `InvalidShape`, `UnknownTarget`, `AppendOnlyViolation`. |
| Enforced by | Storage: UNIQUE `(feed_id, external_id)`, `observations_currency_valid`, `observations_corroborate_valid`, append-only triggers, open-commit trigger. The kernel **MUST** compare `row_hash` before insert so an identical re-import does not depend on catching the UNIQUE failure and does not seal an empty commit (§2.2). U21 does not rewrite observations. |

`payload_json`, descriptions, and hashes **MUST NOT** appear in the error or the log. The ids are the feed id, the external id, and the observation id.

### 3.7 Settlement groups: draft, reconcile, dissolve

State is the latest `settlement_group_states` row. Legal transitions: the first row is `draft` or `reconciled`; `draft` may repeat; `draft → reconciled`; `draft → dissolved`; `reconciled → dissolved`. Nothing leaves `dissolved`. `seq` is contiguous from 1.

A posting belongs to at most one group, ever (`settlement_postings.posting_id` UNIQUE). ADR 0006's word "active" is tightened by that UNIQUE and by delta §5 Q3. See §8.3.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. **Draft:** participants are observations or expected-slot JSON, never both, and the group has no `settlement_postings` in this transaction. An expected slot is a descriptor, never a committed clearing entry. **Reconcile:** this same transaction inserts the `reconciled` state and the group's postings. Postings are on current, non-reversal events. Every clearing account in the group nets to zero per `(currency, ledger account)` (INV-008, INV-017, AT-030). Residuals do not cancel across groups. Required roles for the recipe are present (INV-008 completeness, U26). **Dissolve a draft:** no postings exist, so no reversal is required. **Dissolve a reconciled group:** this same transaction reverses every leg still in the group, reposts every leg that still needs reconciling, and puts each reposted observed leg into its new reconciled group. A non-voided observation behind a reversed leg must have a current interpreting posting afterward. A voided observation is not reposted. The old posting cannot join the new group. |
| Postconditions | After `commit`, `settlement_group_current` is the new state. Draft review state for a participating observation is `interpreted`. Reconciled review state is `reconciled`. A dissolved reconciled group's old legs are superseded. Each reposted observed leg sits in a new reconciled group. |
| Invariants | INV-006, INV-008, INV-017. ADR 0006. Delta §5 Q3. AT-030. |
| Typed error | `InvalidGroupTransition`, `DraftHasPostings`, `ReconciledGroupEmpty`, `NonzeroClearingResidual`, `PostingAlreadySettled`, `SettlementPostingNotCurrent`, `DissolveWithoutReversal`, `DissolveWithoutRepost`, `LegsNotRegrouped`, `RecipeIncomplete`. |
| Enforced by | Storage: transition trigger, "current posting" trigger, UNIQUE posting id, the draft-participant CHECK (observation xor expected JSON), seal (no postings on a draft, reconciled group has postings, zero clearing residual, dissolve reverses every leg and reposts every non-voided observed leg). U19 **MUST** reject those seal conditions first. Storage does not reject a participant row hung on a reconciled or dissolved state; U19 rejects that as `InvalidGroupTransition`. `LegsNotRegrouped` and `RecipeIncomplete` are U19 only. The seal does not check that the replacement joined a new reconciled group (schema comment at the dissolve check). U21 has no view for a missing regroup; §8.10. |

### 3.8 Obligations

A claim is always against a party (ADR 0003), in a receivable or payable ledger account whose `kind` matches `direction`. It links to the purchase posting or postings it came from (ADR 0001). Remaining amount is derived: `original_minor` minus the sum of settlements whose events are not superseded. It is not a stored column.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. `original_minor > 0`. Opening event is staged in this transaction or is an existing current event. Settlement `kind` is `payment` or `forgiveness`, `amount_minor > 0`, and the new total of current settlements does not exceed `original_minor`. The opening event is not superseded (a voided claim cannot be settled, and an already-settled claim cannot be forgiven further). Forgiveness is a new event (INV-012). The kernel does not choose the category; the caller does. Across two entities, each database holds its own claim, and a partial settlement or forgiveness is mirrored as its own cross-entity recipe, two halves written by the application (ADR 0001, ADR 0004). |
| Postconditions | After `commit`, `obligation_balances.remaining_minor` matches that derivation. The claim row is never updated. A further settlement stages another row. |
| Invariants | ADR 0001. INV-012. INV-013 when a correction touches the claim. |
| Typed error | `ClaimAccountMismatch`, `ClaimSettlementExceedsOpen`, `UnknownTarget`, `AppendOnlyViolation`, `CommitSealed`. |
| Enforced by | Storage: `obligations_account_valid`, `obligation_settlements_cap`, append-only triggers. U19 for INV-013 (`TransitiveInvalid`) when a correction would leave the claim inconsistent. |

### 3.9 Recipe-idempotent event writes (one half of a cross-entity recipe, ADR 0004)

The kernel does not orchestrate cross-entity work. The application writes each half of a cross-entity recipe into its own database through `append_event` (§3.3) in that database's own transaction. There is no cross-database message table and no atomicity across databases (ADR 0004). This operation is the kernel's part: storing the half, and making a retry safe.

**Recipe fields.** Two nullable columns on `accounting_events`, both set or both NULL:
- `recipe_instance_id` (ULID). This is the shared recipe id that links the two halves. It is not `recipe_id`, which stays the recipe code (`card_purchase`) used with `recipe_version` (AT-012).
- `counterpart_entity_id` (ULID): the other entity's `entity.id`.

**Counterpart expectations.** One or more rows in `event_counterpart_expectations(event_id, currency, expected_amount_minor)`, primary key `(event_id, currency)`. Each row is the amount the other half must carry in that currency on its intercompany receivable/payable for this entity. A half may expect several currencies, and INV-001 still balances each event per currency. The rows are commit-scoped through their event and append-only. The kernel stores them as facts and never computes a half's amount. The application computes it per currency, as the absolute net of the half's postings on `receivable`/`payable` accounts whose `party_id` is the counterpart's `entity.party_id` (ADR 0004). `journal_postings.party_id` is optional on ordinary events, but **on a recipe half every posting on a `receivable` or `payable` ledger account MUST carry a `party_id`**. The kernel rejects a half that breaks this with `InvalidShape` token `recipe_party` (§4.3), refs `ADR 0004`, ids the ledger account, and stages nothing. The kernel checks only that a party is present; whether it is the counterpart's `entity.party_id` is the application's recipe check, which reports a wrong party as a mismatched amount.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. `recipe_instance_id` and `counterpart_entity_id` are both present or both absent, and both are ULIDs. When present, the half stages at least one expectation row, with at most one row per currency, each `expected_amount_minor > 0`. When absent, it stages none. `counterpart_entity_id` is not this database's `entity.id`. The kernel does not check that it names a real entity, because the counterpart is another database; the registry and the recipe check do that. The event is not a reversal: a reversal carries no recipe fields and no expectation rows. |
| Postconditions | **No current event with this `recipe_instance_id`:** if no event at all carries the id, the event is staged like any `append_event`. If only superseded events carry it, outside the replacement step of `correct`, the result is `IdempotencyConflict` and nothing is staged: the half was reversed, so the recipe instance is retired. **A current event with the same recipe payload** (below): nothing is staged for it. The result names the existing event id, and when nothing else was staged it is the §2.2 no-op (no `commits` row, no `seq` bump). **A current event with a different payload:** `IdempotencyConflict`. The whole transaction rolls back, and the stored event is unchanged. |
| Invariants | ADR 0004 (rewritten 2026-11-26). ADR 0002 (append-only; corrections). At most one current event per `recipe_instance_id` in a database. §2.2. INV-001 for the half itself. |
| Typed error | `IdempotencyConflict` (class user), `InvalidShape` tokens `recipe_fields`, `counterpart_self`, `counterpart_expectations`, and `recipe_party`, `InvalidId`, `InvalidMoney`, `InvalidCurrency`, plus every `append_event` error. |
| Enforced by | Storage: the both-or-neither CHECK; the no-recipe-fields-on-a-reversal CHECK; `event_counterpart_expectations` (primary key per currency, amount CHECK, a trigger that rows belong only to a recipe half, append-only and open-commit triggers, and a `doctor_unsealed_rows` branch through the parent event); the `commits_seal` rule that a recipe half has at least one expectation row; the partial UNIQUE index on `recipe_instance_id WHERE superseded_by IS NULL`; and a trigger that `counterpart_entity_id` is not `entity.id`. Kernel write path (no storage backstop): a receivable/payable posting on a half carries a `party_id` (`recipe_party`). The kernel compares the payload before insert, so an identical retry never reaches the UNIQUE failure. |

**Recipe payload.** The payload is compared field by field against the current event and its postings. A hash is not needed, because both sides are in this database:
- `recipe_id` and `recipe_version`
- `effective_date` and `description`
- `recipe_instance_id` and `counterpart_entity_id`
- the set of expectation rows `(currency, expected_amount_minor)`
- the multiset of postings `(ledger_account_id, currency, side, amount_minor, party_id, memo)`

The caller's event id and posting ids, `commit_id`, and `recorded_at` are not part of the payload. A retry may carry fresh ids, and a match returns the stored ids. Tags, observation links, groups, and claim rows staged with the half are compared by the operations that own them, not by this rule.

**Corrections.** `correct` (§3.4) on a half stages, in its existing order: the reversal (no recipe fields), the `superseded_by` update on the target, then the replacement. The replacement **MUST** carry the same `recipe_instance_id` and `counterpart_entity_id` as the target. Because the update runs before the replacement insert, the partial UNIQUE index never sees two current events. The replacement may carry different expectation rows, including a different set of currencies. The application always corrects both halves, one after the other (Doug, 2026-11-26). The recipe check reports a mismatched amount during the brief window between them, and that is acceptable.

```text
synthetic: Household half R1 (counterpart D, expectation USD 120000) is current
retry R1, same payload, fresh event id   → no new row; returns existing event id;
                                           if nothing else staged, §2.2 no-op
retry R1, debit 120001                   → IdempotencyConflict, ROLLBACK
retry R1, extra expectation EUR 5000     → IdempotencyConflict, ROLLBACK
R1 reversed with no replacement, retry R1 → IdempotencyConflict (instance retired)
```

The public error for a conflict carries the recipe-instance id and the existing event id. It does not carry amounts.

### 3.10 `assert_balance`

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. The financial account is in this database. `cutoff` is an occurred date. `balance_minor` is signed in the account's normal direction (positive means an asset held or a liability owed; a card credit balance is negative). `currency` equals the account's currency. `source` is `manual` or `statement`; a statement source has a `statement_id`. A revision of an earlier assertion is a new row with `supersedes_id`, not an update (INV-011). The closed-month rule in §3.4 applies to every other event in this same transaction. |
| Postconditions | After `commit`, the new row exists and the superseded row, if any, is unchanged. No balance assertion row is ever updated or deleted. Month-1 `assert_balance` does **not** fail just because a statement match was not attempted (MAP: statements are not a feed yet). When U32 is present, exact statement agreement is `StatementNotAgreed` (INV-010), using the occurred-date window plus the in-flight explanation in delta §3.4. |
| Invariants | INV-011. INV-010 is U32. ADR 0002 closed month, enforced on journal commits, not by rewriting assertions. |
| Typed error | `AppendOnlyViolation`, `CurrencyMismatch` (class user, §4.1), `InvalidDate`, `InvalidShape`, `ClosedMonthViolation` if this transaction also posts into a cutoff it would move, `StatementNotAgreed` only from U32. |
| Enforced by | Storage: append-only triggers, `supersedes_id` UNIQUE, source/statement CHECK. There is no storage trigger that the assertion currency matches the account; U19 rejects `CurrencyMismatch` (§8.11). Closed month is U19 only. |

`valuation_snapshots` are not assertions and are not postings. Storage rejects them on anything other than a non-cash asset account, and rejects a currency that is not the account's (`ValuationNotAllowed`, ADR 0005, INV-011).

### 3.11 `commit`

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. The pragma still reads 1. No active write freeze (§3.13), unless this commit is `clear_write_freeze`. U19 has accepted the staged changeset, including every seal condition, the closed-month rule, INV-013, recipe completeness, and regrouping. `expected_revision` stored on the new row is the caller's integer. `seq = expected_revision + 1`. `recorded_at` equals the timestamp fixed at `begin_transaction`, and every staged event carries that same timestamp. `kind` is one of `setup`, `import`, `interpret`, `settle`, `correct`, `assert`, `cross_entity`, `grant`, `freeze`. There is no `void` kind: voiding an observation seals as `import` (§3.5). `kind` labels the commit; the seal checks invariants, not kind. `freeze` is the kind of a `kernel_state` commit (§3.13, §8.13). `docs/schema-v2.sql` lists `freeze` in the `commits.kind` CHECK (U17). |
| Postconditions | If the transaction staged no row because every subject was an identical re-import or an identical recipe retry, `commit` does not insert a `commits` row. The result is `IdempotentNoop` (§2.2). Otherwise either the `commits` row and every staged row are durable, or none of them are. `seq` is the new entity revision. A later insert that names this `commit_id` fails with `CommitSealed`. Dropping the transaction without `commit` leaves nothing. |
| Invariants | AT-013, AT-032. ADR 0002. INV-001. |
| Typed error | Any staging error, rolled back. `RevisionMismatch` if `seq` is not `expected_revision + 1`. `RecordedAtMismatch`. `PartialCommit` (bugcheck) if `commit` had staged rows and returns without a `commits` row, or the deferred foreign key fails because the kernel omitted the seal. The §2.2 no-op is not `PartialCommit`: it returns success and writes no `commits` row on purpose. `ValidatorMissed` (bugcheck) if the seal raises a condition U19 was required to reject. `ForeignKeysDisabled`. |
| Enforced by | kernel for the caller's revision and the pragma. U19 before the insert. Storage seal as the backstop, then `ValidatorMissed` if it fires. Deferred foreign keys at `COMMIT` for a missing seal. |

### 3.12 `doctor`

The checks are read-only. They do not open a write transaction, they do not repair rows, and they do not clear a freeze. It runs on app open, after a cross-entity commit, and before a backup (ADR 0004). Exit status is 0 only when every check below is clean, including no active freeze row. Otherwise the exit status is non-zero. Persisting a missing freeze is a later `record_write_freeze` in its own commit (§3.13), not a write inside the checks.

| Check | Clean when | Finding |
|---|---|---|
| `PRAGMA integrity_check` | the result is `ok` | `IntegrityFailed` (bugcheck) |
| `PRAGMA foreign_key_check` | no rows | `ForeignKeyCheckFailed` (bugcheck) |
| `doctor_unsealed_rows` | no rows | `UnsealedRows` (bugcheck) |
| `doctor_unbalanced_events` | no rows | `DoctorDrift` (bugcheck) |
| `doctor_superseded_drift` | no rows | `DoctorDrift` (bugcheck) |
| `doctor_observation_double_consumed` | no rows | `DoctorDrift` (bugcheck) |
| Evidence bytes against stored hashes | every committed evidence hash matches its bytes | `EvidenceHashMismatch` (bugcheck) |
| Staged evidence objects with no committed row | none, or they are only reported for collection | `OrphanEvidence` (class import) |
| Active write freeze | no `kernel_state` row with `state_key = 'freeze'` is the latest such row and `active = 1` | `WriteFrozen` (bugcheck), `refs` carrying the stored code |

| | |
|---|---|
| Preconditions | A readable handle. `foreign_keys` **MAY** be off; doctor still reports `UnsealedRows`. Doctor reads only this database. It does not open counterpart databases or scan a filesystem for other entity files (ADR 0004 rewrite). When a bugcheck-class finding other than an existing freeze row must be persisted, the caller supplies `expected_revision` for that `record_write_freeze`. A clean doctor does not require the argument. The check statements do not read it. |
| Postconditions | The checks change no row and open no write transaction. The application calls `doctor` before the first write after `open` (ADR 0004: at least on app open). Doctor reports an active freeze row when one exists. It does not clear that row, and a later clean doctor does not clear it either. Clearing is `clear_write_freeze` (§3.13). After the checks, if a bugcheck-class finding is present and no active freeze row exists, the kernel calls `record_write_freeze(expected_revision)`, which seals the freeze in its own commit. `CrossEntityGap` and `OrphanEvidence` do not freeze a database that is otherwise clean: a missing counterpart is an allowed window (ADR 0004) and is surfaced as an open item. The exit status is still non-zero. |
| Invariants | ADR 0002 amendment (flag rebuilt from `reverses_id`). ADR 0004. INV-001, INV-007, INV-020. The freeze lives in `kernel_state` (§8.13). |
| Typed error | The findings above. The read-only checks do not return `StaleRevision`. If the following `record_write_freeze` is given a stale or missing revision, that call returns `StaleRevision` or `MissingExpectedRevision`, the findings are still returned, and no freeze row is written. |
| Enforced by | U21 for the checks. `record_write_freeze` for the persist. These views and pragmas are the check, not a second writer. |

Ownership (ADR 0010 §6, ADR 0011): the kernel runner owns the integrity, foreign-key, unsealed (kernel tables), unbalanced, superseded-drift, and freeze rows. `doctor_observation_double_consumed`, the evidence rows, and the context-table part of `doctor_unsealed_rows` are contributed by the observation import context (U22) and, for refund links and tags, by their contexts.

Write refusal does not delete or update the sealed journal. Recovery from `IntegrityFailed`, `DoctorDrift`, `UnsealedRows`, or `EvidenceHashMismatch` is a restore of a prior consistent snapshot, then a fresh `doctor`. If a freeze row is still active and every other bugcheck-class check is clean, `clear_write_freeze` is what resumes writes. A snapshot that has no freeze row needs no clear. The kernel **MUST NOT** compact, rewrite, or "fix" `superseded_by` in place. A clean doctor report alone does not resume writes while the freeze row is active.

### 3.13 Write freeze (`kernel_state`) and `clear_write_freeze`

The bugcheck write freeze is one place: the entity SQLite database, table `kernel_state`, the latest row whose `state_key` is `freeze`. It is not process memory, not a sidecar file, and not a row the application stores outside the entity database. It survives a process restart because it is in the entity file.

The row:

| Column | Rule |
|---|---|
| `id` | ULID. Caller-supplied or taken from the injected id source. |
| `state_key` | Always `freeze`. |
| `active` | `1` freezes writes. `0` is a clear. The latest row, by its commit's `seq`, is the live state. |
| `code` | A bugcheck code from §4.2. On a clear, the code copied from the active row being cleared. |
| `commit_id` | The freeze commit. Deferred foreign key to `commits`, same pattern as every other commit-scoped row, so the row cannot persist without its `commits` row. |
| `recorded_at` | The freeze commit's timestamp. |

No amounts, memos, names, descriptions, `payload_json`, or `row_hash` are stored on this row.

`record_write_freeze(expected_revision, code)` seals that row in its **own** commit:

1. The changeset that hit the bugcheck has already been rolled back, or doctor has finished its read-only checks. This call opens a new `begin_transaction(expected_revision)`. The integer is the caller's. The kernel does not read `max(seq)` to invent it.
2. `BEGIN IMMEDIATE`, then the freeze read in §2. If an active freeze row is already present, roll back and return that row's `commit_id` and `seq`. Write nothing. Do not seal a second freeze commit.
3. Otherwise compare `expected_revision`, stage one `kernel_state` row with `active = 1` and this `code`, and insert the `commits` row last, `kind = 'freeze'`, `seq = expected_revision + 1`. `COMMIT`.
4. If this commit cannot be sealed (the file is corrupt, or the revision is stale), return that error, write nothing, and still return the original bugcheck. The next `doctor` with a current `expected_revision` retries the persist.

`ForeignKeysDisabled` does not write a freeze row. The pragma is per connection (§3.1). The connection already refuses writes.

These findings do write a freeze when no active row exists: `ValidatorMissed`, `PartialCommit`, `UnmappedStorageError`, `DoctorDrift`, `IntegrityFailed`, `ForeignKeyCheckFailed`, `UnsealedRows`, `EvidenceHashMismatch`. `CrossEntityGap` and `OrphanEvidence` do not. `StorageFault` on a file that cannot be opened cannot persist a row; the error is still returned.

Every later `begin_transaction` checks the latest freeze row under the `BEGIN IMMEDIATE` it just took, before the revision compare and before any changeset write (§2). The exceptions are the two in §2: `clear_write_freeze` continues, and `record_write_freeze` returns the existing freeze commit without writing. Every other operation returns `WriteFrozen` and rolls back. `refs` on that error is the stored `code`. `ids` may include the freeze row id and its `commit_id`. Reads, including `doctor`, stay available.

`clear_write_freeze(expected_revision)` is the only way to clear the freeze. A clean `doctor` does not clear it.

| | |
|---|---|
| Preconditions | Caller-supplied `expected_revision`. An active freeze row exists. This call first runs the §3.12 checks read-only. Every bugcheck-class check other than the freeze row itself is clean. The active freeze row is the thing this call clears, so its presence does not block the clear. `CrossEntityGap` and `OrphanEvidence` may still be open; they do not block the clear. |
| Postconditions | On success, a new `kernel_state` row with `active = 0` is durable in its own `kind = 'freeze'` commit, `commits` row written last, `seq = expected_revision + 1`. The previous freeze row is not updated or deleted. The journal is not modified. The next `begin_transaction` does not return `WriteFrozen`. If any other bugcheck-class check is dirty, the call returns that finding, writes nothing, and the active row stays. |
| Invariants | Append-only journal (ADR 0002) is untouched. Caller-supplied revision, checked inside `BEGIN IMMEDIATE`. Deferred `commit_id` foreign key. `commits` row last. |
| Typed error | `WriteFrozen` is not returned for this operation. `StaleRevision`, `MissingExpectedRevision`, `ForeignKeysDisabled`, plus any bugcheck-class finding that makes the clear illegal. |
| Enforced by | Kernel. The clear is refused in kernel code when doctor is not clean. No trigger in the current schema implements this table (§8.13). |

### 3.14 Context participants: `stage` and `validate` (ADR 0011, decided)

Context rows commit **in the kernel's transaction**: observation and evidence links, refund links, tags, and any other context-owned row. There is no second transaction and no second database file (ADR 0011, Doug 2026-11-26, option (i)). The kernel knows only the shape below. It never interprets a context row.

**Registration.** The application passes `open` (§3.1) a fixed, ordered list of participants. Each participant declares three things:

- a stable participant id, such as `observation_import`, `recipes` or `tags`;
- the tables it owns (ADR 0011 ownership table);
- the §4 codes it may return.

The list does not change for the life of the handle. The kernel calls participants in registration order.

```text
trait ContextParticipant {
  id() -> ParticipantId
  owned_tables() -> [TableName]
  stage(tx: &mut ContextTx, staged: KernelStaged, payload: Option<ParticipantPayload>) -> Result<StageOutcome, Error>
  validate(tx: &ContextTx, set: CommitStagedSet) -> Result<(), Error>
}
KernelStaged      { op: append | correct, commit_id, recorded_at,
                    event_id, line_ids (stored ids, in the caller's line order),
                    correct only: target_event_id, reversal_event_id }
ParticipantPayload  opaque to the kernel; the caller attaches it per participant id to append_event / correct;
                    it refers to lines by position in the event's line list, never by caller-sent ids
StageOutcome      { rows_staged: count, subjects: [Subject] }   // subjects feed the §2.2 no-op result
CommitStagedSet   every event, line, reversal and superseded target staged in this transaction, plus this participant's own staged rows
```

**When `stage` runs.** It runs once per participant, after each `append_event` or `correct` has staged its kernel rows and before that operation returns. For `correct`, it runs after the replacement is staged.

- `line_ids` are the **stored** ids. For an identical §3.9 retry, these are the existing event's ids.
- A participant with no payload for that operation is still called. It may stage nothing.
- `correct` is where a context re-links to the replacement lines. Links to the target's lines stay for history.

**Context-only operations.** `import_observations`, `void_observation`, `put_evidence`, `link_evidence`, and a later tag write use the same `ContextTx`. They get it from an open `begin_transaction` through the participant's own operation. They are not callbacks.

- Such an operation **MAY** call kernel operations on the same transaction. `void_observation` does not: it only inserts `observation_voids` (§3.5).
- `stage` and `validate` **MUST NOT** call kernel operations. There is no re-entry.

**When `validate` runs.** It runs once per participant at `commit` (§3.11):

1. after the kernel's own validation (U19);
2. before the kernel inserts the `commits` row.

It is read-only. It checks the participant's rules over the whole staged set. Examples:

- observation import: INV-007 match on live links, not voided, observed legs reposted on dissolve (ADR 0006). A void does not reverse kernel events;
- recipes: INV-015 cap, refund shape, and INV-013 for refund links on a correction.

**`ContextTx` scope.**

- **Lifetime.** It is valid only inside the open write transaction that created it (`BEGIN IMMEDIATE`, after the freeze check and the revision compare, §2). It cannot outlive `commit` or rollback.
- **Writes.** INSERT only, and only into the participant's declared tables. Every commit-scoped row uses `tx.commit_id()`, and every `recorded_at` is `tx.recorded_at()`. Rows are append-only: no UPDATE or DELETE, and removal is a new row.
- **Reads.** It **MAY** SELECT any table in this entity database.
- **Forbidden.** Writes to kernel tables (including `commits` and `kernel_state`), another participant's tables, DDL, `PRAGMA`, `ATTACH`, `BEGIN`, `COMMIT`, `ROLLBACK`, and `SAVEPOINT`. The kernel **SHOULD** enforce this with the SQLite authorizer callback for the duration of each participant call. Otherwise it is a contract the participant's tests must cover.
- **Who gets one.** Only registered in-process participants receive a `ContextTx`. External callers still never get a connection or send SQL (§1).

**§2.2 no-op.** A transaction is an idempotent no-op only when the kernel staged no row **and** every participant reported `rows_staged = 0`. `IdempotentNoop` returns the kernel's subjects plus every participant's subjects. An example is the stored observation's original `commit_id` and `seq`.

**Error mapping.**

- `stage` and `validate` return a closed §4 `Error`: code, class, refs, ids. A participant returns only codes it declared, or shared codes such as `CurrencyMismatch`, `InvalidShape`, `UnknownTarget`, `DuplicateId`, `InvalidId` and `InvalidDate`. The kernel adds `ADR 0011` to `refs` and returns the error unchanged otherwise.
- A SQLite error on a participant's `ContextTx` is mapped through §4.3, the same closed table. The rows ADR 0011 marks as context-owned are matched there.
- A code the participant did not declare is a bugcheck, `UnmappedStorageError`, with `refs` `ADR 0011`.
- **Any** error from `stage` or `validate` rolls back the whole transaction. No kernel row and no context row persists (AT-013). A user or import class error writes no freeze.
- If the `commits_seal` clause or a storage trigger on a context table raises a rule the participant's `validate` was required to reject first (§4.3 "If the seal raises it" column), the result is `ValidatorMissed`. Its `refs` name the rule and `ADR 0011`, and it is handled like U19's `ValidatorMissed` (§5).

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. Participants registered at `open`. |
| Postconditions | Kernel rows and context rows of one transaction are durable together at `commit`, or none are. |
| Invariants | AT-013. ADR 0002 (append-only). ADR 0011. |
| Typed error | Any code a participant declared, `ValidatorMissed`, `UnmappedStorageError`. |
| Enforced by | Kernel (call order, rollback, no-op rule, authorizer). Each participant for its own rules. Storage triggers on context tables as the backstop. |

## 4. Typed errors

The public error is a closed value:

```text
code     one name from the tables below
class    user | import | bugcheck
refs     zero or more of INV-xxx, ADR 000n, AT-xxx, PR-19
ids      ULIDs, feed ids, external ids, recipe-instance ids, counterpart entity ids
```

`user` means the caller can change the changeset and retry. `import` means the feed row or the other entity's leg is the problem. `bugcheck` means the kernel is in a state it was required to make impossible.

The SQL text in this section is the mapping key for the adapter. It **MUST NOT** be copied into the public error or the log. Logs are `code`, `class`, `refs`, and `ids` only.

### 4.1 User-correctable and import errors

| Code | Class | Meaning |
|---|---|---|
| `MissingExpectedRevision` | user | The caller did not supply `expected_revision`. The kernel did not read one. |
| `StaleRevision` | user | AT-032. After `BEGIN IMMEDIATE`, `max(seq)` is not the caller's integer. |
| `RevisionMismatch` | user | `seq` is not `expected_revision + 1`. |
| `WriteLocked` | user | This database already has a write transaction. |
| `DatabaseUninitialized` | user | A journal operation ran before the singleton entity existed. |
| `Unbalanced` | user | INV-001. An event does not balance per currency. |
| `InsufficientPostings` | user | INV-002. An event has fewer than two postings. |
| `ClosedMonthViolation` | user | ADR 0002. A touched assertion on or after the earliest effective date would change. U19 only. |
| `NonLeafCategory` | user | INV-003. Income or expense posting on a non-leaf. |
| `CategoryParentInvalid` | user | INV-003. Parent is missing, a different entity or type, or already has postings. |
| `LedgerKindMismatch` | user | ADR 0008. Ledger `kind` does not match the financial account kind. |
| `CurrencyMismatch` | user | INV-005, as a validation rejection. Posting, observation, or assertion currency is not the account's. The class is `user` on every operation, including `import_observations` and `assert_balance`. It is not class `import` and it is not a bugcheck. |
| `RecordedAtMismatch` | user | An event's `recorded_at` is not the commit's. |
| `ReversalTargetInvalid` | user | Target is a reversal, is already superseded, or the reversal date is not the target's date. |
| `ReversalDoesNotCancel` | user | Reversal postings do not exactly cancel the target. |
| `SupersedeInvalid` | user | `superseded_by` changed a fact, was cleared, was set twice, or was set outside the reversal's open commit. |
| `SupersedeMissing` | user | A reversal commit did not flag its target. |
| `EventInsertedSuperseded` | user | An insert carried `superseded_by`. |
| `VoidedObservation` | user | A posting link targeted a voided observation. |
| `ObservationAlreadyVoided` | user | A second tombstone for the same observation. |
| `ObservationMismatch` | user | INV-007. Link does not match account, amount, currency, and direction. |
| `ObservationDoubleConsumed` | user | INV-007. Two current events interpret one observation. |
| `ImportConflict` | import | Same `(feed_id, external_id)`, different `row_hash`. Stored row kept. |
| `CorroborationMismatch` | import | `corroborates_id` is a different financial account. |
| `DraftHasPostings` | user | INV-006. |
| `ReconciledGroupEmpty` | user | INV-008. Reconciled state with no postings. |
| `NonzeroClearingResidual` | user | INV-008, INV-017, AT-030. |
| `PostingAlreadySettled` | user | ADR 0006. That posting id is already in a group. |
| `InvalidGroupTransition` | user | ADR 0006. Illegal state or `seq`. |
| `SettlementPostingNotCurrent` | user | ADR 0006. Superseded or reversal posting joined a group. |
| `DissolveWithoutReversal` | user | ADR 0006, delta §5 Q3. |
| `DissolveWithoutRepost` | user | ADR 0006, delta §5 Q3. |
| `LegsNotRegrouped` | user | U19. Reposted observed legs are not in a new reconciled group. |
| `RecipeIncomplete` | user | INV-008 completeness. U19 with U26. |
| `ClaimAccountMismatch` | user | ADR 0001. Direction does not match receivable/payable kind. |
| `ClaimSettlementExceedsOpen` | user | ADR 0001. Over the open amount, or the claim is voided. |
| `RefundShapeInvalid` | user | ADR 0008. Not an opposite-side posting to the purchase's category. |
| `RefundCapExceeded` | user | INV-015. |
| `AppendOnlyViolation` | user | UPDATE or DELETE the schema forbids, including an in-place rename. |
| `CommitSealed` | user | Insert into a commit that already has its `commits` row. |
| `RenameTargetMissing` | user | `reference_renames` target does not exist. |
| `UnknownTarget` | user | Foreign key target is missing. |
| `UnknownRecipe` | user | `recipe_id` + `recipe_version` is not registered. |
| `SecondEntity` | user | ADR 0004. A second `entity` row. |
| `ValuationNotAllowed` | user | ADR 0005, INV-011. Snapshot on a cash, card, or liability account, or wrong currency. |
| `IdempotencyConflict` | user | A recipe write names a `recipe_instance_id` whose current event has a different recipe payload, or whose only events are superseded (§3.9). Reject and write nothing. An identical payload is the §2.2 success, not this code. |
| `TransitiveInvalid` | user | INV-013. A dependent group, claim, or refund would be left invalid. |
| `InvalidId` | user | ULID check failed. |
| `DuplicateId` | user | A primary key or unique id the caller supplied is already present. |
| `InvalidMoney` | user | Non-positive magnitude, or a REAL in an INTEGER money column. Does not apply to signed `balance_minor`. |
| `InvalidCurrency` | user | Currency is not three uppercase letters. |
| `InvalidDate` | user | Date or timestamp check failed. |
| `InvalidShape` | user | Some other CHECK failed. `token` is from the closed list in §4.3. |
| `StatementNotAgreed` | user | INV-010. Raised by U32 only. Month-1 `assert_balance` does not raise it. |
| `OrphanEvidence` | import | Staged evidence bytes with no committed row. `doctor` only. Collectable. |

**Owners (ADR 0011).** The list stays one closed list (the shared error envelope). Codes owned by the observation import context: `ImportConflict`, `CorroborationMismatch`, `ObservationMismatch`, `ObservationDoubleConsumed`, `VoidedObservation`, `ObservationAlreadyVoided`, `OrphanEvidence`, `EvidenceHashMismatch` (§4.2). Codes owned by recipes: `RefundShapeInvalid`, `RefundCapExceeded`. `CurrencyMismatch` stays shared.

An identical re-import and an identical recipe retry are successes, not codes. When the transaction staged nothing else they are the §2.2 no-op: the caller gets the original commit, and no new `commits` row exists. A different payload on the same current `recipe_instance_id` is `IdempotencyConflict`.

### 4.2 Bugchecks

| Code | Meaning |
|---|---|
| `ForeignKeysDisabled` | `PRAGMA foreign_keys` is not 1. Writes are refused. |
| `UnsealedRows` | `doctor_unsealed_rows` returned a row: a commit-scoped row whose `commits` row is missing. |
| `PartialCommit` | `commit` had staged rows and ended without a `commits` row, or a deferred `commit_id` foreign key failed because the kernel omitted the seal. The §2.2 no-op is not this code. |
| `ValidatorMissed` | `commits_seal` raised a condition §3 requires U19 to reject first. `refs` names that condition. |
| `DoctorDrift` | `doctor_unbalanced_events`, `doctor_superseded_drift`, or `doctor_observation_double_consumed` returned a row on a database the kernel sealed. |
| `IntegrityFailed` | `PRAGMA integrity_check` was not `ok`. |
| `ForeignKeyCheckFailed` | `PRAGMA foreign_key_check` returned a row on a connection that had foreign keys on. |
| `EvidenceHashMismatch` | A committed evidence hash does not match its bytes, or the bytes are missing. |
| `UnmappedStorageError` | SQLite raised a constraint the tables in §4.3 do not name. The taxonomy is closed, so this is a kernel bug, not a new public code. |
| `StorageFault` | The driver failed (I/O, corrupt open) before a typed rule could run. |
| `WriteFrozen` | An active `kernel_state` freeze row exists (§3.13). Later writes are refused. `refs` is the stored bugcheck code. Reads still run. |

### 4.3 Storage messages mapped to codes

RAISE text is matched as a substring. The adapter maps it and then discards it.

| RAISE substring | Code | If the seal raises it after U19 accepted |
|---|---|---|
| `AT-032: stale revision` | `StaleRevision` | `ValidatorMissed` |
| `event recorded_at must equal` | `RecordedAtMismatch` | `ValidatorMissed` |
| `INV-002: every event needs at least two postings` | `InsufficientPostings` | `ValidatorMissed` |
| `INV-001: event does not balance` | `Unbalanced` | `ValidatorMissed` |
| `reversal does not exactly cancel` | `ReversalDoesNotCancel` | `ValidatorMissed` |
| `must be flagged superseded_by` | `SupersedeMissing` | `ValidatorMissed` |
| `INV-006: draft groups never hold postings` | `DraftHasPostings` | `ValidatorMissed` |
| `INV-008: a reconciled group must hold` | `ReconciledGroupEmpty` | `ValidatorMissed` |
| `nonzero clearing residual` | `NonzeroClearingResidual` | `ValidatorMissed` |
| `dissolving a reconciled group must reverse` | `DissolveWithoutReversal` | `ValidatorMissed` |
| `observed legs must be reposted` | `DissolveWithoutRepost` | `ValidatorMissed` |
| `only superseded_by may be set` | `SupersedeInvalid` | user error (not a seal check) |
| `targets a current non-reversal event and keeps its effective date` | `ReversalTargetInvalid` | user error |
| `an event is inserted current` | `EventInsertedSuperseded` | user error |
| `leaf category` | `NonLeafCategory` | user error |
| `category parent must be` | `CategoryParentInvalid` | user error |
| `ledger kind does not match` | `LedgerKindMismatch` | user error |
| `posting currency must match` | `CurrencyMismatch` | user error |
| `observation currency must match` | `CurrencyMismatch` | user error. Class stays `user` when the operation is `import_observations`. |
| `corroborating row must be` | `CorroborationMismatch` | import |
| `posting must match its observation` | `ObservationMismatch` | user error |
| `voided observation cannot be interpreted` | `VoidedObservation` | user error |
| `invalid settlement group transition` | `InvalidGroupTransition` | user error |
| `only current, non-reversal postings` | `SettlementPostingNotCurrent` | user error |
| `receivable/payable ledger account` | `ClaimAccountMismatch` | user error |
| `settlement exceeds the open claim` | `ClaimSettlementExceedsOpen` | user error |
| `opposite-side posting to the purchase` | `RefundShapeInvalid` | user error |
| `INV-015: refund links exceed` | `RefundCapExceeded` | user error |
| `valuation snapshots only on non-cash` | `ValuationNotAllowed` | user error |
| `rename target does not exist` | `RenameTargetMissing` | user error |
| `commit already sealed` | `CommitSealed` | user error |
| `are never updated` or `are never deleted` or `is immutable` or `is never deleted` or `are immutable` | `AppendOnlyViolation` | user error |

SQLite constraint text that is not a RAISE. The first matching row wins. A ULID check and a hash check both contain `GLOB`; they are matched by the rows below, not by the date row. The date row matches `date(` and the timestamp suffix `*Z'`.

| SQLite substring | Code |
|---|---|
| `seq = expected_revision + 1` | `RevisionMismatch` |
| `UNIQUE constraint failed: entity.singleton` | `SecondEntity` |
| `UNIQUE constraint failed: observations.feed_id, observations.external_id` | `ImportConflict` |
| `UNIQUE constraint failed: settlement_postings.posting_id` | `PostingAlreadySettled` |
| `UNIQUE constraint failed: observation_voids.observation_id` | `ObservationAlreadyVoided` |
| `UNIQUE constraint failed: accounting_events.recipe_instance_id` | `IdempotencyConflict`. The kernel compares the payload before insert, so an identical retry never reaches this failure. |
| `ADR 0004: counterpart_entity_id must be another entity` | `InvalidShape` token `counterpart_self` |
| `CHECK constraint failed` on the recipe both-or-neither or reversal rule | `InvalidShape` token `recipe_fields` |
| `counterpart expectations belong to a recipe half` | `InvalidShape` token `counterpart_expectations` |
| `a recipe half needs at least one counterpart expectation` | `InvalidShape` token `counterpart_expectations` (U19 rejects first; from the seal it is `ValidatorMissed`) |
| `UNIQUE constraint failed: event_counterpart_expectations.event_id, event_counterpart_expectations.currency` | `InvalidShape` token `counterpart_expectations` |
| `CHECK constraint failed` on `expected_amount_minor > 0` | `InvalidMoney` |
| `UNIQUE constraint failed: commits.seq` | `RevisionMismatch` |
| `UNIQUE constraint failed: accounting_events.reverses_id` | `ReversalTargetInvalid` |
| `UNIQUE constraint failed: settlement_group_states.group_id, settlement_group_states.seq` | `InvalidGroupTransition` |
| `UNIQUE constraint failed:` ending in `.id`, or any other `UNIQUE constraint failed:` | `DuplicateId` when the conflict is an id column; otherwise `InvalidShape` token `unique_other` (`entity.party_id`, `tags.name`, `evidence_deletions.sha256`, `balance_assertions.supersedes_id`, `user_grants.revokes_id`, `refund_links.refund_posting_id, refund_links.purchase_posting_id`) |
| `FOREIGN KEY constraint failed` and this `commit` never inserted its `commits` row | `PartialCommit` (bugcheck) |
| `FOREIGN KEY constraint failed` after the kernel inserted that row, and U19 had accepted every target | `ValidatorMissed` (bugcheck) |
| any other `FOREIGN KEY constraint failed` | `UnknownTarget`. U19 returns `UnknownRecipe` itself when the missing target is `recipes`; SQLite's foreign-key text does not name the column, so the storage mapping stays `UnknownTarget`. |
| `cannot store REAL value in INTEGER column` | `InvalidMoney` |
| CHECK text containing `amount_minor` | `InvalidMoney` |
| CHECK text containing `length(id) = 26` or `[^0-9A-HJKMNP-TV-Z]` | `InvalidId` |
| CHECK text containing `currency` | `InvalidCurrency` |
| CHECK text containing `date(` or `*Z'` | `InvalidDate` |
| CHECK text containing `last4` | `InvalidShape` token `last4` |
| CHECK text containing `sha256` or `row_hash` | `InvalidShape` token `hash` |
| any other `CHECK constraint failed` whose expression is one of the tokens below | `InvalidShape` |
| any other `CHECK constraint failed`, or any other SQLite error string | `UnmappedStorageError` (bugcheck) |

`InvalidShape` tokens, and only these: `commit_kind`, `grant_role`, `void_reason`, `observation_direction`, `settlement_state`, `obligation_direction`, `settlement_kind`, `recipe_fields`, `counterpart_self`, `counterpart_expectations`, `recipe_party`, `assertion_source`, `valuation_source`, `feed_kind`, `account_type`, `ledger_kind`, `financial_kind`, `tracking_mode`, `party_label`, `posting_side`, `evidence_link_target`, `draft_participant`, `grant_revoke_pair`, `period_order`, `last4`, `hash`, `unique_other`.

`recipe_party` has no RAISE or CHECK behind it. The kernel write path raises it directly (§3.9): a posting on a `receivable` or `payable` ledger account in a recipe half has no `party_id`. It is listed here so the token list stays the one closed list.

**Moved (ADR 0011).** These rows and tokens stay in this closed table, but their RAISE/CHECK sources are context-owned: tokens `void_reason`, `observation_direction`, `feed_kind`, `evidence_link_target`, `hash` (observation import); `unique_other` from `tags.name` (application), from the `refund_links` pair (recipes), and from `evidence_deletions.sha256` (observation import); the RAISE rows `observation currency must match`, `corroborating row must be`, `posting must match its observation`, `voided observation cannot be interpreted`, `observed legs must be reposted` (observation import), and `opposite-side posting to the purchase`, `INV-015: refund links exceed` (recipes).

A new CHECK or RAISE lands only by extending this section in the same change (§7).

## 5. Bugcheck policy

A bugcheck is an impossible state. The examples that define it:

- `PRAGMA foreign_keys` is not 1 on a connection the kernel will write (PR 19 fix 1). The kernel refuses every write. The smoke script shows why: with the pragma off, an orphan commit-scoped row is accepted and skips the seal.
- A trigger that U19 was required to reject first fires anyway (`ValidatorMissed`). The validator and the seal disagree, or the kernel skipped the validator.
- `doctor` finds drift, unsealed rows, a foreign-key violation, an integrity failure, or an evidence hash mismatch on a database the kernel is responsible for.
- `commit` would end with staged rows and no `commits` row (`PartialCommit`).

Fail closed, every time:

1. Roll back the open transaction. No partial commit (AT-013).
2. Refuse further writes on that database by the active `kernel_state` freeze row (§3.13). That row is in the entity SQLite database, written in its own commit, and it is what a restart reads. The refusal is not stored on the process handle and not in a file beside the database. Reads stay available so `doctor` can be repeated. `doctor` reports the row and does not clear it.
3. Exit non-zero when the process is `doctor` or a command-line entry that hit the bugcheck.
4. The message and the log contain `code`, `class=bugcheck`, `refs`, and `ids` only. No amounts, memos, names, descriptions, `payload_json`, `row_hash`, account numbers, `last4`, SQL text, or row images.

The kernel **MUST NOT** delete the offending rows, recompute `superseded_by` in place, or commit a compensating entry as part of handling the bugcheck. Append-only still holds (ADR 0002). A person restores a prior snapshot.

`StaleRevision`, `Unbalanced`, `ClosedMonthViolation`, and `CurrencyMismatch` are ordinary user errors when `begin_transaction` or U19 returns them. `CurrencyMismatch` stays class `user` on import as well (§4.1, §6). They become a bugcheck only as the underlying code of `ValidatorMissed`, when the layer that should have returned them already said the changeset was clean. `WriteFrozen` is a bugcheck because the database is already frozen, not because the caller's currency or balance failed validation.

A missing recipe half is not a kernel finding at all. Two databases cannot commit together, and a window where one half is missing is visible and expected (ADR 0004). The application's recipe check reports it. The per-database `doctor` neither sees it nor freezes for it.

## 6. Triage

Classify from the returned `code` and `class`. Do not re-derive the class from the English text of a log line.

| What you see | Class | Owner | What to capture |
|---|---|---|---|
| `class=user` from §4.1, including `CurrencyMismatch` from `import_observations`, `assert_balance`, or a posting | User error. A validation rejection. The changeset or the revision the caller sent is illegal. `CurrencyMismatch` means the currency is not the account's. The class does not become `import` and does not become a bugcheck because the operation was an import. | The recipe or the UI that built the changeset. On `import_observations` the adapter built a row whose currency is not the account's; the class is still `user`. `RecipeIncomplete` is the recipe author (U26) together with U19. `IdempotencyConflict` is the application that retried a recipe half with a different payload or a retired `recipe_instance_id` (§3.9). | `code`, `refs`, operation name, caller-supplied `expected_revision`, commit id if one was allocated, target event or group or observation ids, idempotency key. Not the amount and not the payload hash. |
| `ImportConflict`, `CorroborationMismatch`, `OrphanEvidence` | Data or import issue. The books are unchanged (`ImportConflict`). `CurrencyMismatch` is not in this class. | The adapter that minted the natural key. Not the kernel. Recipe-check findings (missing half, counterpart not visible, orphaned for 7 days, duplicate, mismatched counterpart or amount) and `RegistryMismatch` are application findings (ADR 0004), not kernel codes. | Feed id, external id, observation id, recipe-instance id, counterpart entity id. |
| `class=bugcheck`, including `WriteFrozen` | Kernel bug, or a write refused because that bug is already stored as the freeze row. A guard that should have made the original finding unreachable did not. | Kernel. `ValidatorMissed` and the revision test are U19. `doctor` findings of drift, unsealed rows, integrity, and foreign keys are U21. `ForeignKeysDisabled` is the kernel `open` path. `WriteFrozen` means the `kernel_state` freeze row is active (§3.13); capture the stored code from `refs`. | `code`, `refs`, the doctor view or pragma name, commit id, event ids, the freeze row id, and the test name when this came from CI. |

Evidence rules for every class:

- Ids and codes only. The same ban as §5.
- Do not attach a query result, a CSV row, a screenshot of a ledger, or a payload.
- Do not read or copy a private data directory or an attachments folder to "explain" the failure. Reproduce with a synthetic fixture that uses the same code path and invented ids.
- A `user` or `import` error leaves the previous sealed revision as the live revision. Say that revision integer. Do not paste the rejected changeset.
- A bugcheck also names the last sealed `seq` if `commits` is readable. If it is not, say so and stop.

Working as designed, and not one of the three classes: an identical re-import that adds no row and seals no commit (§2.2), an identical recipe retry that likewise seals no commit, a register that hides a superseded pair, a balance that still includes both sides of a reversed pair, and a negative `balance_minor` on a card credit balance. A recipe retry with a different amount, or any other payload difference, is `IdempotencyConflict`, not this list.

## 7. Admission test

A new recipe or a new kernel operation is not ready to land until every line below is true. The change that adds the operation updates this spec in the same change when the contract is new.

1. **Contract.** The operation has a §3 row: preconditions, postconditions, invariants (INV or ADR), the typed error for each violation, and where it is enforced (kernel, storage, U19, or U21).
2. **Closed taxonomy.** Every failure path returns a code already in §4, or this spec adds the code and its RAISE or CHECK mapping in the same change. No new public string. An unmapped SQLite error is `UnmappedStorageError`, and that is a failed admission, not a shippable behavior.
3. **Coverage.** A smoke check in `docs/schema-v2-smoke.sh`, an acceptance test (AT-xxx), or both, covers the success path and each reject path. A storage rule has a smoke check. The revision contract keeps the kernel-level test in §2.1, not only the SQL smoke.
4. **Single writer, caller-supplied revision.** The operation runs inside `begin_transaction(expected_revision)`. It does not open its own transaction, it does not call `max(seq)` to invent `expected_revision`, and `commit` still writes the caller's integer with `seq = expected_revision + 1`. The §2.2 no-op is the exception that writes no `commits` row. The freeze check under `BEGIN IMMEDIATE` still runs (§3.13).
5. **Foreign keys.** `open` still asserts `PRAGMA foreign_keys = 1` and still refuses writes otherwise. The new operation does not use a second connection that skips the assert.
6. **Synthetic fixtures only.** Invented ULIDs and invented amounts. No real statements, no copied feed payloads, no private data directory, no attachments folder.
7. **Quiet failures.** Tests assert on `code`, `class`, `refs`, and ids. They do not snapshot amounts, memos, names, or payloads into logs.
8. **Corrections and dissolves.** If the operation corrects or dissolves a reconciled group, the tests cover set-once `superseded_by`, a reversal that keeps the target's effective date, correction-of-a-correction reversing the replacement, and a dissolve that reverses, reposts, and regroups in the same commit. A void test covers a tombstone that leaves kernel events and lines unchanged, and origin derived from live links (§3.5).
9. **Doctor.** The success fixture leaves every §3.12 check clean, including no active freeze row. An injected bugcheck-class fault exits non-zero, leaves no partial journal rows, persists the freeze with `record_write_freeze` in its own commit when `expected_revision` is current, and does not print a row. A later clean doctor reports the freeze and does not clear it. `clear_write_freeze` is what removes the freeze, and only when the checks are clean.

Recipes compile to these operations. A recipe that needs a journal shape this contract cannot express does not get a private write path. It fails admission until the contract grows.

## 8. Disagreements resolved

These are places the sources do not say the same thing. The choice used in this spec is stated so U19 does not have to guess.

### 8.1 Where this spec lives

`accounting-core/docs/KERNEL_INTERFACE_SPEC-v1-SUPERSEDED.md` (named `accounting-core/docs/KERNEL_INTERFACE_SPEC.md` until 2026-11-26) already exists. It specifies the v1 `LedgerStore` crate: opaque entities, a contra role, `replaces_entry_id`, one database, and a bugcheck that includes partition deltas and the requested mutation. The crate still implements that file. This change does not modify crate code, so that file is left as the crate's contract.

This v2 interface is `docs/KERNEL_INTERFACE_SPEC.md`, next to schema v2. v1's bugcheck payload is not the v2 policy. v2 messages are ids and codes only (§5).

### 8.2 How wide the kernel is

ADR 0007 says the kernel knows nothing about tags, match hints, visibility, or feeds, and that the application owns cross-entity orchestration and `doctor`. Schema v2 stores feeds, observations, tags, and grants in the entity database and defines the doctor views. The U16 assignment listed import, the cross-entity protocol of that time, and `doctor` as kernel operations.

Choice: match hints, feed formats, and viewer-relative display stay outside the kernel. Import of already-keyed observations and `doctor` are kernel operations because the schema and this assignment put their rules here. Superseded 2026-11-26 for the cross-entity protocol: the ADR 0004 rewrite drops it. The kernel's cross-entity part is now only the recipe-idempotent event write (§3.9). Orchestration, the registry, and the cross-database recipe check belong to the application, and `doctor` reads one database. The kernel stores tag rows and does not interpret them. ADR 0007 now records this choice (amended 2026-11-25). §0 and §1 use this same widened scope. §8 may widen a kernel, not only tighten one; when it does, the ADR is amended so the documents agree. Narrowed 2026-11-26 by §8.16 (ADR 0011): import, observation links, evidence, refund links, and tags leave the kernel.

### 8.3 Settlement exclusivity is forever

ADR 0006 says a posting belongs to at most one active group. Schema v2 declares `settlement_postings.posting_id` UNIQUE with no lifetime, and delta §5 Q3 says a surviving leg could never join another group, so dissolving a reconciled group reverses and reposts every leg in the same commit.

Choice: uniqueness is forever. The replacement posting joins the new group. U19 regroups those legs. That is a requirement, not a choice left to U35.

### 8.4 Balances sum every posting

The glossary says balances ignore `superseded_by`. Read carelessly, that drops superseded postings from the sum. ADR 0002 and the `account_balances` view sum every posting and never read the flag. A reversal nets to zero against its target.

Choice: balances are the sum of all postings. The flag hides pairs from the register. It is not an input to the balance.

### 8.5 Magnitudes are positive

SPEC INV-002 says a nonnegative magnitude. Schema v2 checks `amount_minor > 0`, and the delta says magnitudes are positive.

Choice: zero is `InvalidMoney`. Signed values belong only on `balance_minor` and `value_minor`.

### 8.6 The journal is append-only

SPEC §7 still describes a mutable current-state ledger plus before/after audit rows. The delta replaces that section and says an ADR beats the delta, which beats SPEC. ADR 0002 is append-only. `audit_operations` and `audit_changes` are not in schema v2.

Choice: a correction appends a reversal, sets `superseded_by` once, and appends a replacement. SPEC §7 steps 1 to 6 survive only as the delta rewrote them: `begin_transaction(expected_revision)`, and otherwise commit nothing.

### 8.7 Occurred date assigns the period

SPEC §3.4 reconciles on the posting date and allows an effective-date override. The 2026-11-22 decision, the delta, the glossary, and the schema use `effective_date` as the occurred date for every period assignment, including reconciliation. `posted_date` is on observations only.

Choice: follow the delta. There is no `effective_date_override`.

### 8.8 Closed-month earliest date

ADR 0002 says a commit is checked against assertions on or after "its effective date". Delta §7 and §5 Q2 say a correction uses `min(reversal date, replacement date)`, because the reversal keeps the target's date and the replacement may carry a corrected date. There is no trigger for this.

Choice: U19 uses that minimum, which is the minimum `effective_date` among the events in the commit. `ClosedMonthViolation` has no storage backstop.

### 8.9 A cross-entity gap does not freeze a clean database

§5 says a bugcheck refuses writes. ADR 0004 says a missing counterpart is a visible window, and restoring one database at a time is allowed.

Choice (revised 2026-11-26 for the ADR 0004 rewrite): a missing, duplicate, or mismatched recipe half is an application recipe-check finding. It is not a kernel code and never writes a freeze row. The per-database `doctor` does not see other databases. Intra-database drift, unsealed rows, integrity, foreign keys, and evidence hash failures still refuse writes. The refusal is the active `kernel_state` freeze row in that entity database (§3.13, §8.13). `clear_write_freeze` is the only clear.

### 8.10 Regrouping has no doctor view yet

The seal checks that a dissolved reconciled group's legs are reversed and that non-voided observed legs are reposted. The schema comment leaves regrouping to U19. No doctor view names a reposted leg that never joined a new reconciled group.

Choice: `LegsNotRegrouped` is a U19 error. This spec does not pretend U21 already detects it. A later doctor view has to be added explicitly and pass §7.

### 8.11 Assertion currency

Postings and observations have currency triggers. `balance_assertions` does not. A wrong-currency assertion would make the closed-month recompute meaningless.

Choice: U19 rejects it as `CurrencyMismatch`, class `user`, on every operation including `import_observations` and `assert_balance` (§4.1, §6). It is a validation rejection: the currency is not the account's. It is not class `import` and not a bugcheck. That is an interface rule ahead of storage, not a trigger this spec invents.

### 8.12 INV-009 and month-1 INV-010

The delta drops the movement `unverified → verified` state. Corroboration or statement coverage replaces it. MAP says month 1 does not require a statement-balance match.

Choice: there is no kernel operation for INV-009. `StatementNotAgreed` is reserved for U32 and is not raised by month-1 `assert_balance`. The closed-month check still runs whenever an assertion already exists.

### 8.13 Where the bugcheck write freeze is stored

§3.12 says doctor changes no row. §5 says a bugcheck refuses later writes. Those two sentences do not by themselves say where the refusal is remembered after a restart.

Choice: one location. The freeze is the latest `kernel_state` row with `state_key = 'freeze'` and `active = 1`, in the entity SQLite database (§3.13). `record_write_freeze` writes it in its own commit: caller-supplied `expected_revision`, checked under `BEGIN IMMEDIATE`, the `kernel_state` row staged, the `commits` row written last with `kind = 'freeze'` and `seq = expected_revision + 1`, deferred `commit_id` foreign key. Doctor's checks stay read-only and report the row. `clear_write_freeze` appends `active = 0` in its own commit and is the only clear. The journal stays append-only. This is not in-process handle state and not a record outside the entity database.

`docs/schema-v2.sql` contains `kernel_state`, and `commits.kind` includes `freeze` (U17). Append-only and open-commit triggers on `kernel_state` reuse the RAISE substrings `are never updated`, `are never deleted`, and `commit already sealed`, so §4.3 needs no new code. `doctor_unsealed_rows` includes the table. `commits_seal` permits a commit that contains no events; a freeze commit relies on that, and on the deferred `commit_id` foreign key, the same rule as every other commit-scoped row.

### 8.14 Recipe retry payload

The earlier cross-database apply and its payload hash are gone (ADR 0004 rewrite). The rule it carried is unchanged in spirit: a retry is a no-op only when it is the same write, and any difference is a conflict that writes nothing.

Choice: §3.9 compares the recipe payload field by field against the current event with that `recipe_instance_id`. Equal: write nothing, return the existing event id, and, when nothing else was staged, take the §2.2 no-op with no empty commit. Different, or only superseded events carry the id: `IdempotencyConflict`, roll back, write nothing. The expectation rows are part of the payload. A retry that adds, drops, or changes one currency's expected amount is a different payload.

### 8.15 Entity identity is the database, not a column

Schema v2 put `entity_id` on `ledger_accounts` and `journal_postings`, with a trigger that a posting's entity owns its ledger account, and INV-001 was stated per `(entity, currency)`. ADR 0004 already says one database holds one entity, so that column could only ever hold one value.

Choice (Doug, 2026-11-26; ADR 0004 and ADR 0008):
- **No per-row `entity_id`.** INV-001 is per event and per currency. INV-003 is leaf-only, and the same-type parent rule stays.
- **The singleton `entity` row** (`id`, `name`, `party_id`, `singleton = 1`) is the database's one identity record. It is what another entity's `counterpart_entity_id` names, and what the application's registry check compares.
- **Cross-entity identity on an event** is `counterpart_entity_id` (§3.9). The kernel checks only that it is not this database's own `entity.id`.
- **No `target_entity_id`.** The ADR 0004 rewrite removed the cross-database apply, so this field and its target check are gone.
- **`EntityMismatch` is retired** from the kernel taxonomy. Its posting meaning went with row-level `entity_id`, and its apply-target meaning went with the cross-database apply. Writing to the wrong database is now prevented before the kernel is called: the application opens the target, compares `entity.id` and `entity.party_id` with its registry, and refuses with the application code `RegistryMismatch`. That code is not in §4, because it is not a kernel error.

`docs/schema-v2.sql` (PR 31, merged) has no row-level `entity_id`. It carries the recipe columns on `accounting_events` and the `event_counterpart_expectations` table (§3.9), so the schema and this section agree.

### 8.16 The kernel stages only events and their lines (ADR 0011)

Doug, 2026-11-26: tags, evidence (file) links, observation links, and refund links are not kernel concepts. They live in context tables in the same entity database that reference a kernel event or line (posting) by id. `append_event` stages the event and its postings only; an event's recipe-half fields and counterpart expectations are part of the event (§3.9).

Choice: owners are observation import (`feeds`, `import_runs`, `observations`, `observation_voids`, `posting_observations`, evidence tables and `evidence_links`), recipes (`refund_links`), and the application (`tags`, `posting_tags`). `docs/schema-v2.sql` is unchanged; its triggers on those tables are storage backstops owned by those contexts. Decided (Doug, 2026-11-26, ADR 0011 option (i)): context rows commit in the kernel's transaction through the two-method participant hook in §3.14. A separate post-commit transaction and a separate application database were rejected.

## 9. Changelog

### 2026-11-27 — `VoidWithoutReversal` leaves the closed list

`void_observation` tombstones an observation and does not reverse a kernel event, so nothing produces `VoidWithoutReversal`. The code is removed from §4.1 and from the observation-import owner list. The Rust enum drops the same variant.

### 2026-11-26 — ADR 0011: kernel stages only events and lines

Docs only. Option (i) decided by Doug: §3.14 (new) makes the context participant hook normative (`stage`, `validate`, `ContextTx` scope, no-op rule, error mapping). §1, §3.3, §3.4, §3.5, §3.6, §3.12, §4.1, §4.3 and §8.2 now mark tags, observation links, refund links, evidence links, and observation import as context-owned, with pointers to ADR 0011; §8.16 records the choice. No code, schema, or smoke change.

### 2026-11-26 — ADR 0009 accepted, not yet in this contract

Docs only. ADR 0009 (single-currency accounts and one FX Clearing account, Doug 2026-11-26) is a post-month-1 unit. Until it is scheduled, this contract, `docs/schema-v2.sql` and the smoke checks are unchanged, and §0 item 1 does not pull its rules in early. When it lands, §3.3, §4.1 and §4.3 gain its rules in the same change (§7): `CurrencyMismatch` widens to every ledger account except FX Clearing, and `InvalidShape` gains `account_currency` and `fx_clearing`. The proposed schema delta is `docs/SPEC-v2-delta.md` section 6.

### 2026-11-26 — post-merge fixes to the PR 32 text

Spec only. Flagged by Backend after PR 31 and PR 32 merged.
- **§8.15** no longer says `event_counterpart_expectations` is missing from the schema. PR 31 added it to `docs/schema-v2.sql`.
- **§3.9 and §4.3:** the rule that a recipe half's receivable/payable postings carry a `party_id` is now stated in §3.9, and its rejection gets its own `InvalidShape` token, `recipe_party`. The kernel already rejects this case as `InvalidShape` with refs `ADR 0004` and no token. The code change to attach the token, and the test that today asserts no token, are a Backend follow-up.

### 2026-11-26 — multi-currency recipe halves; Doug's PR 32 answers

Spec only. The schema change is the PR 32 patch for PR 31.
- **Columns dropped.** `counterpart_expected_amount_minor` and `counterpart_currency` come off `accounting_events`. `recipe_instance_id` and `counterpart_entity_id` stay, both set or both NULL.
- **New table** `event_counterpart_expectations`, one row per currency, primary key `(event_id, currency)`, amount > 0.
  - Commit-scoped through its event and append-only. `doctor_unsealed_rows` covers it, which makes 23 open-commit tables.
  - Rows are only allowed on a recipe half, so a reversal has none.
  - `commits_seal` requires at least one row per recipe half.
  - The rows are part of the §3.9 payload.
- **The recipe check** (application) compares per currency. On each half, the postings on the intercompany receivable/payable for the counterpart are checked against the other half's expectation for that currency (ADR 0004). INV-001 stays per currency. `InvalidShape` gains `counterpart_expectations`.
- **The U17 entry below** says 22 open-commit tables and "recipe-half columns". Both describe PR 31 before this change.

### 2026-11-26 — ADR 0004 rewrite and row-level entity_id removed (Doug)

Spec only. `docs/schema-v2.sql` is unchanged here; the schema change is carried separately.

- **Row-level `entity_id` removed.** INV-001 is per currency, and the same-entity rule is leaf-only (§3.3, §4.1, §4.3, §8.15).
- **Cross-database message tables removed.** §3.9 is now recipe-idempotent event writes:
  - two nullable recipe columns on `accounting_events` (`recipe_instance_id`, `counterpart_entity_id`)
  - per-currency rows in `event_counterpart_expectations`
  - a partial UNIQUE index on current `recipe_instance_id`
  - an identical retry, expectation rows included, is a no-op; a different payload or a retired instance is `IdempotencyConflict`

  A reversal carries no recipe fields or expectation rows, and a replacement keeps the recipe fields.
- **Doctor is single-database.** `doctor` lost its cross-database pairing row. `CrossEntityGap` left the kernel taxonomy; recipe-check findings are application findings.
- **`EntityMismatch` and `target_entity_id` retired.** The application registry check returns `RegistryMismatch`.
- **Other updates:** `InvalidShape` loses the old cross-entity action token and gains `recipe_fields`, `counterpart_self`, and `counterpart_expectations`. §1, §2.2, §6, §8.2, §8.9, and §8.14 follow.

### 2026-11-26 — six inconsistencies flagged on PR 26

Spec only. `docs/schema-v2.sql` is unchanged. Schema v2 invariants are unchanged: append-only journal, `commits` row written last, deferred foreign keys, `commits_seal`, and caller-supplied `expected_revision` checked inside `BEGIN IMMEDIATE`.

1. **§0 matches §8.2.** Authority item 1 no longer says §8 may only tighten an ADR. Import, the cross-entity protocol of that time, and `doctor` are named as kernel operations, and the amended ADR 0007 is the same scope.
2. **§1 is the widened kernel.** The boundary prose lists those operations and retires the narrow reading (journal, invariants, and the commit validator only).
3. **No empty commit on a pure retry.** §2.2: an identical re-import or an equal-payload cross-entity retry that stages nothing rolls back, inserts no `commits` row, does not bump `seq`, and returns `IdempotentNoop` with the original `commit_id` and `seq`.
4. **One freeze location.** §3.13 and §8.13: the active `kernel_state` / `freeze` row in the entity database, written in its own commit, checked under `BEGIN IMMEDIATE` before any write. Doctor reports it. `clear_write_freeze` clears it.
5. **`CurrencyMismatch` is class `user` everywhere.** §4.1, §4.3, §6, and §8.11. It is a validation rejection, including on `import_observations`. It is not class `import` and not a bugcheck.
6. **Retry payload conflict.** §3.9 and §8.14: a retry with a different payload, including a different amount, is `IdempotencyConflict`. The kernel rejects, writes nothing, and does not overwrite. (The hashed apply this item described was later removed by the ADR 0004 rewrite; §3.9 now compares the recipe payload.)

`docs/adr/0007-layering.md` has a 2026-11-26 clarification so it does not keep the withdrawn "writes resume at the next clean doctor" sentence or treat every import and cross-entity apply as a commit that always seals. No other document was edited.

### 2026-11-26 — U17 schema is in this branch

`kernel_state`, `commits.kind = 'freeze'`, and the ADR 0004 recipe-half columns are in `docs/schema-v2.sql` on this branch. §3.11 and §8.13 say the schema already lists `freeze`. The open-commit count is 22. The wording of the six sections that overlapped PR 32 is PR 32's, apart from those two freeze sentences.
