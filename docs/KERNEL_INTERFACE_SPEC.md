# Kernel interface specification

**Status:** Proposed (U16, 2026-11-26). Docs only. No implementation in this change.
**Storage:** `docs/schema-v2.sql` (migration 1). Smoke backstop: `docs/schema-v2-smoke.sh` (synthetic data only).
**Normative neighbors:** `docs/SPEC-v2-delta.md` (especially §3 and §5), `docs/adr/0001`–`0008`, `docs/SPEC.md`, `GLOSSARY.md`, `docs/design/MAP.md`.

**MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative. This file is the contract for the per-entity kernel that sits on schema v2. It is not a recipe catalog and it is not a storage driver.

## 0. Authority

Where two documents disagree, use this order, then the choices in §8:

1. An ADR, unless §8 records a later decision that changes it. The change may tighten a rule or widen the kernel. Widening is what §8.2 did, and it is the scope this file uses: importing already-keyed observations, the outbox row in the first leg's commit, the idempotent inbox apply, and `doctor` are kernel operations. ADR 0007 was amended on 2026-11-25 so it states that same wider scope. §0 does not treat the wider scope as a conflict with the ADR. A later §8 decision that widens the kernel is resolved the same way: amend the ADR until the two documents name the same operations.
2. `docs/SPEC-v2-delta.md` over `docs/SPEC.md`. The delta already says: where the delta and an ADR disagree, the ADR wins and the delta has a bug.
3. `docs/schema-v2.sql` for any rule that is actually a trigger, CHECK, UNIQUE, foreign key, or STRICT table. If this spec describes a trigger the schema does not have, this spec has a bug.
4. This file over `accounting-core/docs/KERNEL_INTERFACE_SPEC.md` for the v2 kernel. That other file is the v1 `LedgerStore` contract of the `accounting-core` crate. It is not updated here, and it is not this interface. See §8.1.

The kernel scope under this order is the widened one in §1 and §8.2. Match hints, feed formats, and viewer-relative display stay outside the kernel. The earlier narrow reading of ADR 0007 (journal, invariants, and the commit validator only) is not the contract.

Synthetic fixtures only. Invented ULIDs, invented external ids, invented amounts. No real statements, no feed files, no rows from a private data directory, and nothing from an attachments folder. Identifiers in this text are revision integers and error codes, not sample books.

## 1. Boundary

One SQLite database holds one entity (ADR 0004). Foreign keys never cross databases. A counterpart entity is a `parties` row whose id is that entity's stable `party_id` (ADR 0003). There is no `visibility` column.

The kernel is the only writer of journal, observation, settlement, obligation, assertion, and outbox/inbox rows. Its operations are the append-only journal and commit seal, the numbered invariants, `import_observations`, the outbox row in the first leg's commit, the idempotent inbox apply, and `doctor` (§8.2). It stores feed rows, tag rows, and access grants as facts and does not interpret them. Callers hand it a changeset. They do not get a connection and they do not send SQL.

| In the kernel | Outside the kernel |
|---|---|
| Append-only journal, commit seal, numbered invariants the schema enforces | Match hints and suggestion ranking (ADR 0006, ADR 0007) |
| Observations as already-keyed facts (`feed_id` + `external_id`) | Feed file formats and how an adapter derives an external id (ADR 0007) |
| Tag rows on postings | What a tag means to a person |
| Outbox row in the same commit as the first leg; idempotent inbox apply on the other database | Deciding to start the counterpart leg, and any UI around a gap (ADR 0004) |
| `doctor` over one or more entity databases | Viewer-relative presentation (ADR 0003) |

That scope is the same one §8.2 and ADR 0007 (amended 2026-11-25, clarified 2026-11-26) record. Match hints, suggestion ranking, feed file formats, how an adapter derives an external id, and viewer-relative presentation stay outside. The earlier narrow kernel (journal, invariants, and the commit validator only; no import, no outbox or inbox, no `doctor`) is not the contract.

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
4. `commit` runs the commit validator (U19), then inserts the `commits` row **last**, with `expected_revision` equal to the caller's integer and `seq = expected_revision + 1`. A CHECK enforces that equality. The kernel stores the caller's integer unchanged. It does not substitute `current`. §2.2 is the exception: an identical re-import or an equal-payload inbox retry that staged no row inserts no `commits` row and does not take the next `seq`.
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

1. Database sealed at revision 4. Call `begin_transaction(3)`. The result is `StaleRevision`. The `commits` count is unchanged. No new event, grant, observation, or outbox row exists. A kernel that does `expected_revision = max(seq)` inside the transaction would seal revision 5 and this assertion fails.
2. Database sealed at revision 4. Call `begin_transaction(4)`, stage a trivial legal changeset, `commit`. The new row has `expected_revision = 4` (the argument) and `seq = 5`.
3. The test observes the integer bound into the `commits` insert. It equals the caller's argument, not a fresh query result substituted for that argument.
4. `open` on a connection whose `PRAGMA foreign_keys` does not return 1 returns `ForeignKeysDisabled`. A following `begin_transaction` or `commit` writes nothing. Reads still work, so `doctor` can see damage.

Messages and assertions in that test contain ids and codes only.

### 2.2 Idempotent no-op: no empty commit

An identical re-import (§3.6, same `(feed_id, external_id)` and same `row_hash`) or an inbox retry whose payload hash equals the stored row (§3.9) is a no-op when the transaction staged no other row.

The kernel has already taken `BEGIN IMMEDIATE` and compared the caller-supplied `expected_revision`. A mismatch is still `StaleRevision` and writes nothing. On a match the kernel does **not** insert a `commits` row, does **not** allocate `seq = expected_revision + 1`, and does **not** seal an empty commit. It rolls the write transaction back. Deferred foreign keys and `commits_seal` do not run, because no `commits` row is inserted. The journal is unchanged.

`commit` returns that outcome. The caller observes it as success, not as an error code. Dropping the transaction without `commit` also leaves no new row, and does not return this result.

- The result is `IdempotentNoop` with `noop = true`. `subjects` lists every already-stored subject, in request order. A subject is either `(feed_id, external_id)` or an `idempotency_key`, plus the `commit_id` and `seq` of the commit that first stored it. That pair is the original commit result. `commit` is the call that returns it.
- `coalesce(max(commits.seq), 0)` is unchanged and still equals the `expected_revision` passed into `begin_transaction`. No `commits` row from this call exists. A later `begin_transaction` with the same integer is not `StaleRevision` because of this call.
- The stored observation or inbox row is byte-for-byte the original. Its `commit_id` is the one named in `subjects`.

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

Stages one non-reversal event and its postings. A reversal is `correct`, not this operation.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. Caller-supplied ids are 26-character Crockford base32 ULIDs, or the kernel allocates them from the injected source. `recipe_id` + `recipe_version` already exist (AT-012). At least two postings. Each posting has one ledger account, one currency, an explicit `debit` or `credit`, and `amount_minor > 0`. Per currency inside the event, sum of debits equals sum of credits. Postings carry no `entity_id`: every ledger account in the database is the entity's (ADR 0004 amendment 2026-11-26). An income or expense posting uses a leaf category. A posting on a financial-account-backed ledger account uses that account's currency. `recorded_at` is the transaction's fixed timestamp. `effective_date` is an occurred date (`YYYY-MM-DD`). `superseded_by` is null. A `posting_observations` link matches the observation's financial account, amount, and currency, with inflow as debit and outflow as credit, on both asset and liability accounts (month 1 is exact 1:1, delta §5 Q4). The observation is not voided. The event's commit is still open. |
| Postconditions | The event and its postings, tags, observation links, and refund links are staged in this transaction. They are invisible to other connections until `commit`. Caller-supplied ids are unchanged. Balances, once committed, are the sum of all postings and do not read `superseded_by` (ADR 0002). |
| Invariants | INV-001, INV-002, INV-003, INV-004, INV-005, INV-007, INV-015. ADR 0008 (no contra type; categories are income/expense accounts). ADR 0002 (append-only; an event is inserted current). |
| Typed error | `Unbalanced`, `InsufficientPostings`, `NonLeafCategory`, `CategoryParentInvalid`, `LedgerKindMismatch`, `CurrencyMismatch`, `ObservationMismatch`, `ObservationDoubleConsumed`, `VoidedObservation`, `RefundShapeInvalid`, `RefundCapExceeded`, `UnknownRecipe`, `UnknownTarget`, `InvalidId`, `DuplicateId`, `InvalidMoney`, `InvalidCurrency`, `InvalidDate`, `CommitSealed`, `EventInsertedSuperseded`, `AppendOnlyViolation`. |
| Enforced by | U19 for INV-001 and INV-002 before the seal; the seal raises the same two and is then a `ValidatorMissed` bugcheck. Storage for leaf, currency, refund shape and cap, observation match, voided observation, open commit, and "inserted current". U21 `doctor_unbalanced_events` and `doctor_observation_double_consumed` after the fact; a row there on a database this kernel sealed is `DoctorDrift`. |

Category splits of one observation are other postings on the same event. They are not a second interpreting event for the same bank row (delta §5 Q4).

### 3.4 `correct` (reversal plus replacement)

One transaction. The kernel stages, in this order: the reversal event and its cancelling postings, then `UPDATE accounting_events SET superseded_by = <reversal id> WHERE id = <target>`, then the replacement event and its postings, then any group or claim rows that replacement needs. That UPDATE is the only update the schema accepts.

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. The target is a current event: `reverses_id` is null and `superseded_by` is null. The target is not itself a reversal. The reversal's `effective_date` equals the target's `effective_date`. The reversal's postings exactly cancel the target, account by account and currency by currency (opposite side, same accounts, same amounts). The replacement is a normal `append_event` and **MAY** carry a different `effective_date`. Correcting a correction reverses the replacement, never the earlier reversal (AT-033). `superseded_by` is set once, inside this still-open transaction, to the reversal's id, and is not cleared or pointed at any other event. Every group, obligation, refund link, and assertion the change touches is valid afterward (INV-013). Closed-month: every existing balance assertion with a cutoff on or after the earliest `effective_date` in this transaction, on every financial account the transaction posts to, recomputes to the same `balance_minor`. Earliest date is `min(reversal effective_date, replacement effective_date)` (delta §7, §5 Q2). |
| Postconditions | After `commit`, the target's `superseded_by` is the reversal id. The register (`register_events`) shows the replacement and hides the target and the reversal. Audit still returns every event, with lineage through `reverses_id`. Account balances equal the plain sum of every posting. A second correction of the new replacement repeats the same shape. |
| Invariants | ADR 0002 and its amendment. AT-032, AT-033. INV-001, INV-011, INV-012, INV-013. |
| Typed error | `ReversalTargetInvalid`, `ReversalDoesNotCancel`, `SupersedeInvalid`, `SupersedeMissing`, `EventInsertedSuperseded`, `ClosedMonthViolation`, `TransitiveInvalid`, plus any `append_event` error on the replacement. |
| Enforced by | Storage: `accounting_events_reversal_valid` (current non-reversal, same effective date), `accounting_events_only_supersede` (set once, to the reversing event, inside its open commit), `accounting_events_insert_current`, and the seal (exact cancel, flag present in this commit). U19 **MUST** reject those seal conditions first; a seal raise is `ValidatorMissed`. U19 alone rejects `ClosedMonthViolation` and `TransitiveInvalid` (no trigger). U21 `doctor_superseded_drift` rebuilds the flag from `reverses_id`; a row there is `DoctorDrift`. |

`superseded_by` **MUST NOT** be set in a later transaction. The smoke script covers a second set, a clear, a set after seal, a reversal that never flags its target, a reversal of a reversal, and a reversal whose date differs from the target.

### 3.5 `void_observation(observation_id, reason)`

| | |
|---|---|
| Preconditions | Inside `begin_transaction`. The observation exists and has no `observation_voids` row. `reason` is `duplicate`, `extraction_error`, or `other`. Every current event that interprets the observation (a `posting_observations` link on an event with `superseded_by` null and `reverses_id` null) is reversed in this same transaction, under the `correct` reversal rules, including the target's effective date and the set-once flag. The observation row itself is not deleted. |
| Postconditions | After `commit`, `observation_review_state` is `voided`. A later `posting_observations` insert for this observation fails. Linked current interpretations are reversed. Source facts stay in `observations` (INV-014). |
| Invariants | ADR 0002 void rule. INV-014. |
| Typed error | `VoidWithoutReversal`, `ObservationAlreadyVoided`, `InvalidShape` token `void_reason`, plus reversal errors from §3.4. |
| Enforced by | Storage seal rejects a void whose current linked events are not reversed. U19 **MUST** reject that first (`ValidatorMissed` if the seal raises it). Storage `posting_observations_not_voided` and UNIQUE `observation_voids.observation_id`. There is no DELETE path; DELETE on `observations` is `AppendOnlyViolation`. |

### 3.6 `import_observations`

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
| Preconditions | Inside `begin_transaction`. `original_minor > 0`. Opening event is staged in this transaction or is an existing current event. Settlement `kind` is `payment` or `forgiveness`, `amount_minor > 0`, and the new total of current settlements does not exceed `original_minor`. The opening event is not superseded (a voided claim cannot be settled, and an already-settled claim cannot be forgiven further). Forgiveness is a new event (INV-012). The kernel does not choose the category; the caller does. Across two entities, each database holds its own claim, and a partial settlement or forgiveness is mirrored with the outbox (ADR 0001, ADR 0004). |
| Postconditions | After `commit`, `obligation_balances.remaining_minor` matches that derivation. The claim row is never updated. A further settlement stages another row. |
| Invariants | ADR 0001. INV-012. INV-013 when a correction touches the claim. |
| Typed error | `ClaimAccountMismatch`, `ClaimSettlementExceedsOpen`, `UnknownTarget`, `AppendOnlyViolation`, `CommitSealed`. |
| Enforced by | Storage: `obligations_account_valid`, `obligation_settlements_cap`, append-only triggers. U19 for INV-013 (`TransitiveInvalid`) when a correction would leave the claim inconsistent. |

### 3.9 Cross-entity outbox and inbox (ADR 0004)

Two databases, two transactions. They cannot share one SQLite commit. The first leg and its outbox row commit together in the source database. The counterpart applies the inbox in its own database.

| | |
|---|---|
| Preconditions | **Outbox:** inside the same `begin_transaction` as the source event. `recipe_instance_id` is a ULID. `idempotency_key` is non-empty and unique in this database. `counterpart_party_id` is a party in this database. `action` is `leg`, `partial_settlement`, `forgiveness`, or `correction`. `amount_minor > 0`. The event id is the event staged in this transaction. **Inbox:** `open` on the counterpart database, its own `begin_transaction(expected_revision)` with that database's caller-supplied revision. The apply carries `target_entity_id`, `idempotency_key`, `recipe_instance_id`, `source_party_id`, `action`, `amount_minor`, and `currency`. `target_entity_id` equals this database's `entity.id` (§8.15). |
| Postconditions | Source `commit` durably contains the leg and the outbox row, or neither. Inbox apply first checks `target_entity_id` against `entity.id`. If they differ, it fails closed with `EntityMismatch`: the whole transaction rolls back, no inbox row or event is written, and no `commits` row is sealed. This check runs before the idempotency lookup, so a misrouted retry is never reported as a no-op. Inbox apply then compares the payload hash below before insert. Equal hash: write nothing for that key, do not overwrite the stored row, succeed. When the transaction staged nothing else, the result is the §2.2 no-op and the subject is that key's original `commit_id` and `seq`. Different hash, including a different `amount_minor` or any other payload field: `IdempotencyConflict`, the whole transaction rolls back, the stored inbox row is unchanged, and no `commits` row is written. A first apply of a new key stages one inbox row and the counterpart event. `doctor` is how a missing, duplicate, or mismatched counterpart becomes visible. This operation does not pretend the two commits are one. |
| Invariants | ADR 0004. Idempotency key is the inbox primary key. The payload hash in this section. §2.2. |
| Typed error | `EntityMismatch` (class user), `IdempotencyConflict` (class user), `UnknownTarget`, `InvalidId`, `InvalidMoney`, `InvalidShape` token `outbox_action`, `CommitSealed`. A missing counterpart found later is `CrossEntityGap` from `doctor`, not a failure of the source `commit`. |
| Enforced by | Storage: UNIQUE `idempotency_key`, inbox primary key, action CHECK, append-only triggers (no update, no delete), open-commit triggers. Kernel for the target check and the hash compare before insert, so a retry does not depend on catching the primary-key failure and a conflict does not overwrite. U21 pairs databases by the shared payload fields, not by recipe instance alone. |

**Payload hash.** The payload is these five fields and no others, in this order: `recipe_instance_id`, `source_party_id`, `action`, `amount_minor`, `currency`. `idempotency_key` is the lookup key and is not part of the hashed bytes. The local `event_id`, `commit_id`, and `recorded_at` are assigned on first apply and are not part of the payload. A retry does not supply a replacement event, and a match does not change the stored `event_id`.

The canonical bytes are UTF-8. Each field is written, then one U+000A newline, including a newline after `currency`. `amount_minor` is the decimal integer with no sign and no leading zeros. The other four fields are their stored text with no added spaces. None of the five can contain a newline, so the encoding is injective: different payloads produce different bytes.

`payload_hash` is the lowercase hex SHA-256 of those bytes (64 characters). The kernel computes it from the apply arguments and recomputes it from the stored inbox columns. There is no `payload_hash` column. The comparison is equality of the two digests. Unequal digests are `IdempotencyConflict`. The kernel rejects before `INSERT`, rolls back, and does not update the stored row.

```text
stored: key idem-acme-001, recipe 01ARZ3NDEKTSV4RRFFQ69G5FAV,
        source party 01ARZ3NDEKTSV4RRFFQ69G5FAW (Acme Corp),
        action leg, amount_minor 1500, currency USD
retry amount_minor 1501 → digest differs → IdempotencyConflict, ROLLBACK,
        stored row unchanged, no new commits row
retry with the same five fields → digests equal → no new inbox row;
        if nothing else was staged, §2.2 returns the original commit
```

The public error for a conflict carries the outbox id, the idempotency key, and the recipe-instance id. It does not carry the amount or the payload hash.

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
| Preconditions | Inside `begin_transaction`. The pragma still reads 1. No active write freeze (§3.13), unless this commit is `clear_write_freeze`. U19 has accepted the staged changeset, including every seal condition, the closed-month rule, INV-013, recipe completeness, and regrouping. `expected_revision` stored on the new row is the caller's integer. `seq = expected_revision + 1`. `recorded_at` equals the timestamp fixed at `begin_transaction`, and every staged event carries that same timestamp. `kind` is one of `setup`, `import`, `interpret`, `settle`, `correct`, `void`, `assert`, `cross_entity`, `grant`, `freeze`. `kind` labels the commit; the seal checks invariants, not kind. `freeze` is the kind of a `kernel_state` commit (§3.13, §8.13). The schema CHECK does not list `freeze` yet; the implementation change that adds `kernel_state` extends the CHECK. This spec does not claim the current CHECK already accepts it. |
| Postconditions | If the transaction staged no row because every subject was an identical re-import or an equal-payload inbox retry, `commit` does not insert a `commits` row. The result is `IdempotentNoop` (§2.2). Otherwise either the `commits` row and every staged row are durable, or none of them are. `seq` is the new entity revision. A later insert that names this `commit_id` fails with `CommitSealed`. Dropping the transaction without `commit` leaves nothing. |
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
| Outbox/inbox pairing across databases | every outbox row has exactly one inbox row with the same idempotency key, `recipe_instance_id`, `action`, `amount_minor`, and `currency`. Party ids are not compared across databases. | `CrossEntityGap` (class import) |
| Evidence bytes against stored hashes | every committed evidence hash matches its bytes | `EvidenceHashMismatch` (bugcheck) |
| Staged evidence objects with no committed row | none, or they are only reported for collection | `OrphanEvidence` (class import) |
| Active write freeze | no `kernel_state` row with `state_key = 'freeze'` is the latest such row and `active = 1` | `WriteFrozen` (bugcheck), `refs` carrying the stored code |

| | |
|---|---|
| Preconditions | A readable handle. `foreign_keys` **MAY** be off; doctor still reports `UnsealedRows`. Pairing checks name each counterpart database explicitly. The kernel does not scan a filesystem for other entity files. When a bugcheck-class finding other than an existing freeze row must be persisted, the caller supplies `expected_revision` for that `record_write_freeze`. A clean doctor does not require the argument. The check statements do not read it. |
| Postconditions | The checks change no row and open no write transaction. The application calls `doctor` before the first write after `open` (ADR 0004: at least on app open). Doctor reports an active freeze row when one exists. It does not clear that row, and a later clean doctor does not clear it either. Clearing is `clear_write_freeze` (§3.13). After the checks, if a bugcheck-class finding is present and no active freeze row exists, the kernel calls `record_write_freeze(expected_revision)`, which seals the freeze in its own commit. `CrossEntityGap` and `OrphanEvidence` do not freeze a database that is otherwise clean: a missing counterpart is an allowed window (ADR 0004) and is surfaced as an open item. The exit status is still non-zero. |
| Invariants | ADR 0002 amendment (flag rebuilt from `reverses_id`). ADR 0004. INV-001, INV-007, INV-020. The freeze lives in `kernel_state` (§8.13). |
| Typed error | The findings above. The read-only checks do not return `StaleRevision`. If the following `record_write_freeze` is given a stale or missing revision, that call returns `StaleRevision` or `MissingExpectedRevision`, the findings are still returned, and no freeze row is written. |
| Enforced by | U21 for the checks. `record_write_freeze` for the persist. These views and pragmas are the check, not a second writer. |

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

## 4. Typed errors

The public error is a closed value:

```text
code     one name from the tables below
class    user | import | bugcheck
refs     zero or more of INV-xxx, ADR 000n, AT-xxx, PR-19
ids      ULIDs, feed ids, external ids, idempotency keys, recipe-instance ids
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
| `EntityMismatch` | user | ADR 0004 amendment 2026-11-26. An inbox apply's `target_entity_id` is not this database's `entity.id`. Nothing is written (§3.9, §8.15). |
| `CurrencyMismatch` | user | INV-005, as a validation rejection. Posting, observation, or assertion currency is not the account's. The class is `user` on every operation, including `import_observations` and `assert_balance`. It is not class `import` and it is not a bugcheck. |
| `RecordedAtMismatch` | user | An event's `recorded_at` is not the commit's. |
| `ReversalTargetInvalid` | user | Target is a reversal, is already superseded, or the reversal date is not the target's date. |
| `ReversalDoesNotCancel` | user | Reversal postings do not exactly cancel the target. |
| `SupersedeInvalid` | user | `superseded_by` changed a fact, was cleared, was set twice, or was set outside the reversal's open commit. |
| `SupersedeMissing` | user | A reversal commit did not flag its target. |
| `EventInsertedSuperseded` | user | An insert carried `superseded_by`. |
| `VoidWithoutReversal` | user | A void left a current interpreting event. |
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
| `IdempotencyConflict` | user | Same idempotency key, payload hash differs (§3.9). Reject, write nothing, do not overwrite the stored inbox row. A difference in `recipe_instance_id` is one such mismatch, not the only one. An equal hash is the §2.2 success, not this code. |
| `TransitiveInvalid` | user | INV-013. A dependent group, claim, or refund would be left invalid. |
| `InvalidId` | user | ULID check failed. |
| `DuplicateId` | user | A primary key or unique id the caller supplied is already present. |
| `InvalidMoney` | user | Non-positive magnitude, or a REAL in an INTEGER money column. Does not apply to signed `balance_minor`. |
| `InvalidCurrency` | user | Currency is not three uppercase letters. |
| `InvalidDate` | user | Date or timestamp check failed. |
| `InvalidShape` | user | Some other CHECK failed. `token` is from the closed list in §4.3. |
| `StatementNotAgreed` | user | INV-010. Raised by U32 only. Month-1 `assert_balance` does not raise it. |
| `CrossEntityGap` | import | ADR 0004. Missing, duplicate, or mismatched counterpart. `doctor` only. |
| `OrphanEvidence` | import | Staged evidence bytes with no committed row. `doctor` only. Collectable. |

An identical re-import and an inbox retry whose payload hash matches are successes, not codes. When the transaction staged nothing else they are the §2.2 no-op: the caller gets the original commit, and no new `commits` row exists. A different payload on the same inbox key is `IdempotencyConflict`.

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
| `a void must reverse every current event` | `VoidWithoutReversal` | `ValidatorMissed` |
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
| `UNIQUE constraint failed: cross_entity_outbox.idempotency_key` | `IdempotencyConflict` |
| `UNIQUE constraint failed: cross_entity_inbox.idempotency_key` | `IdempotencyConflict`, unless the kernel already returned success because the payload hash matched (§3.9). The kernel compares the hash before insert, so a match never reaches this failure and a mismatch never overwrites the row. SQLite reports this primary key as a UNIQUE failure, not as the words "PRIMARY KEY". |
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

`InvalidShape` tokens, and only these: `commit_kind`, `grant_role`, `void_reason`, `observation_direction`, `settlement_state`, `obligation_direction`, `settlement_kind`, `outbox_action`, `assertion_source`, `valuation_source`, `feed_kind`, `account_type`, `ledger_kind`, `financial_kind`, `tracking_mode`, `party_label`, `posting_side`, `evidence_link_target`, `draft_participant`, `grant_revoke_pair`, `period_order`, `last4`, `hash`, `unique_other`.

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

`CrossEntityGap` is not a bugcheck. Two databases cannot commit together, and a window where one leg is missing is visible and expected (ADR 0004). `doctor` still exits non-zero.

## 6. Triage

Classify from the returned `code` and `class`. Do not re-derive the class from the English text of a log line.

| What you see | Class | Owner | What to capture |
|---|---|---|---|
| `class=user` from §4.1, including `CurrencyMismatch` from `import_observations`, `assert_balance`, or a posting | User error. A validation rejection. The changeset or the revision the caller sent is illegal. `CurrencyMismatch` means the currency is not the account's. The class does not become `import` and does not become a bugcheck because the operation was an import. | The recipe or the UI that built the changeset. On `import_observations` the adapter built a row whose currency is not the account's; the class is still `user`. `RecipeIncomplete` is the recipe author (U26) together with U19. `IdempotencyConflict` is the caller that retried an inbox key with a different payload (§3.9). | `code`, `refs`, operation name, caller-supplied `expected_revision`, commit id if one was allocated, target event or group or observation ids, idempotency key. Not the amount and not the payload hash. |
| `ImportConflict`, `CorroborationMismatch`, `CrossEntityGap`, `OrphanEvidence` | Data or import issue. The books are unchanged (`ImportConflict`) or a counterpart leg has not landed (`CrossEntityGap`). `CurrencyMismatch` is not in this class. | The adapter that minted the natural key, or the application that orchestrates the outbox (ADR 0004). Not the kernel, unless the kernel wrote both sides and they disagree with their own outbox row, in which case reclassify as `bugcheck` and hand to U19. | Feed id, external id, observation id, idempotency key, recipe-instance id, outbox id, inbox id. |
| `class=bugcheck`, including `WriteFrozen` | Kernel bug, or a write refused because that bug is already stored as the freeze row. A guard that should have made the original finding unreachable did not. | Kernel. `ValidatorMissed` and the revision test are U19. `doctor` findings of drift, unsealed rows, integrity, and foreign keys are U21. `ForeignKeysDisabled` is the kernel `open` path. `WriteFrozen` means the `kernel_state` freeze row is active (§3.13); capture the stored code from `refs`. | `code`, `refs`, the doctor view or pragma name, commit id, event ids, the freeze row id, and the test name when this came from CI. |

Evidence rules for every class:

- Ids and codes only. The same ban as §5.
- Do not attach a query result, a CSV row, a screenshot of a ledger, or a payload.
- Do not read or copy a private data directory or an attachments folder to "explain" the failure. Reproduce with a synthetic fixture that uses the same code path and invented ids.
- A `user` or `import` error leaves the previous sealed revision as the live revision. Say that revision integer. Do not paste the rejected changeset.
- A bugcheck also names the last sealed `seq` if `commits` is readable. If it is not, say so and stop.

Working as designed, and not one of the three classes: an identical re-import that adds no row and seals no commit (§2.2), an inbox retry whose payload hash matches and that likewise seals no commit, a register that hides a superseded pair, a balance that still includes both sides of a reversed pair, and a negative `balance_minor` on a card credit balance. An inbox retry with a different amount, or any other payload-hash mismatch, is `IdempotencyConflict`, not this list.

## 7. Admission test

A new recipe or a new kernel operation is not ready to land until every line below is true. The change that adds the operation updates this spec in the same change when the contract is new.

1. **Contract.** The operation has a §3 row: preconditions, postconditions, invariants (INV or ADR), the typed error for each violation, and where it is enforced (kernel, storage, U19, or U21).
2. **Closed taxonomy.** Every failure path returns a code already in §4, or this spec adds the code and its RAISE or CHECK mapping in the same change. No new public string. An unmapped SQLite error is `UnmappedStorageError`, and that is a failed admission, not a shippable behavior.
3. **Coverage.** A smoke check in `docs/schema-v2-smoke.sh`, an acceptance test (AT-xxx), or both, covers the success path and each reject path. A storage rule has a smoke check. The revision contract keeps the kernel-level test in §2.1, not only the SQL smoke.
4. **Single writer, caller-supplied revision.** The operation runs inside `begin_transaction(expected_revision)`. It does not open its own transaction, it does not call `max(seq)` to invent `expected_revision`, and `commit` still writes the caller's integer with `seq = expected_revision + 1`. The §2.2 no-op is the exception that writes no `commits` row. The freeze check under `BEGIN IMMEDIATE` still runs (§3.13).
5. **Foreign keys.** `open` still asserts `PRAGMA foreign_keys = 1` and still refuses writes otherwise. The new operation does not use a second connection that skips the assert.
6. **Synthetic fixtures only.** Invented ULIDs and invented amounts. No real statements, no copied feed payloads, no private data directory, no attachments folder.
7. **Quiet failures.** Tests assert on `code`, `class`, `refs`, and ids. They do not snapshot amounts, memos, names, or payloads into logs.
8. **Corrections, voids, dissolves.** If the operation reverses, voids, or dissolves a reconciled group, the tests cover set-once `superseded_by`, a reversal that keeps the target's effective date, correction-of-a-correction reversing the replacement, and a dissolve that reverses, reposts, and regroups in the same commit.
9. **Doctor.** The success fixture leaves every §3.12 check clean, including no active freeze row. An injected bugcheck-class fault exits non-zero, leaves no partial journal rows, persists the freeze with `record_write_freeze` in its own commit when `expected_revision` is current, and does not print a row. A later clean doctor reports the freeze and does not clear it. `clear_write_freeze` is what removes the freeze, and only when the checks are clean.

Recipes compile to these operations. A recipe that needs a journal shape this contract cannot express does not get a private write path. It fails admission until the contract grows.

## 8. Disagreements resolved

These are places the sources do not say the same thing. The choice used in this spec is stated so U19 does not have to guess.

### 8.1 Where this spec lives

`accounting-core/docs/KERNEL_INTERFACE_SPEC.md` already exists. It specifies the v1 `LedgerStore` crate: opaque entities, a contra role, `replaces_entry_id`, one database, and a bugcheck that includes partition deltas and the requested mutation. The crate still implements that file. This change does not modify crate code, so that file is left as the crate's contract.

This v2 interface is `docs/KERNEL_INTERFACE_SPEC.md`, next to schema v2. v1's bugcheck payload is not the v2 policy. v2 messages are ids and codes only (§5).

### 8.2 How wide the kernel is

ADR 0007 says the kernel knows nothing about tags, match hints, visibility, or feeds, and that the application owns the outbox and `doctor`. Schema v2 stores feeds, observations, tags, grants, outbox, and inbox in the entity database and defines the doctor views. The U16 assignment lists import, outbox/inbox, and `doctor` as kernel operations.

Choice: match hints, feed formats, and viewer-relative display stay outside the kernel. Import of already-keyed observations, the outbox row, the inbox apply, and `doctor` are kernel operations because the schema and this assignment put their rules here. The kernel stores tag rows and does not interpret them. ADR 0007 now records this choice (amended 2026-11-25). §0 and §1 use this same widened scope. §8 may widen a kernel, not only tighten one; when it does, the ADR is amended so the documents agree.

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

Choice: `CrossEntityGap` exits non-zero and does not refuse writes on a database whose other doctor checks are clean. Intra-database drift, unsealed rows, integrity, foreign keys, and evidence hash failures do refuse writes. The refusal is the active `kernel_state` freeze row in that entity database (§3.13, §8.13). Doctor reports the row. `clear_write_freeze` is the only clear. A later clean doctor does not clear it by itself.

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

`docs/schema-v2.sql` does not yet contain `kernel_state`, and `commits.kind` does not yet include `freeze`. This spec does not describe a trigger the schema already has. The implementation that adds the table extends the kind CHECK in that same change (§7). Until that change, the contract is this section. `commits_seal` already permits a commit that contains no events; a freeze commit relies on that, and on the new table following the same deferred-foreign-key rule as every other commit-scoped row.

### 8.14 Inbox retry payload

§3.9 used to treat a matching `recipe_instance_id` as a successful retry and raised `IdempotencyConflict` only when that id differed. A second apply with the same key and instance but a different amount would then depend on doctor noticing the mismatch later.

Choice: the payload hash in §3.9 is the compare. The same idempotency key with a different amount, or any other difference in `recipe_instance_id`, `source_party_id`, `action`, `amount_minor`, or `currency`, is `IdempotencyConflict`: reject, roll back, write nothing, do not overwrite. An equal hash writes nothing and, when the transaction staged no other row, is the §2.2 no-op that returns the original commit and seals no empty commit.

### 8.15 Entity identity is the database, not a column

Schema v2 put `entity_id` on `ledger_accounts` and `journal_postings`, with a trigger that a posting's entity owns its ledger account, and INV-001 was stated per `(entity, currency)`. ADR 0004 already says one database holds one entity, so that column could only ever hold one value.

Choice (Doug, 2026-11-26; ADR 0004 and ADR 0008 amendments): no per-row `entity_id`. INV-001 is per event and per currency. INV-003 is leaf-only, and the same-type parent rule stays. The singleton `entity` row (`id`, `name`, `party_id`, `singleton = 1`) is the database's one identity record and its self-reference for the outbox and inbox. The outbox keeps `counterpart_party_id` and the inbox keeps `source_party_id`. An inbox apply names `target_entity_id`, and the kernel compares it with `entity.id` before anything else. A mismatch is `EntityMismatch` (class `user`, owner: the application that routed the apply). The kernel writes nothing and seals no commit. `EntityMismatch` no longer means "posting entity does not own the ledger account": that rule is gone, and the storage message `posting entity must own` no longer exists.

`cross_entity_inbox` has no target column, so this check is kernel-only, with no storage backstop. The kernel test that covers it is a wrong-target apply that returns `EntityMismatch`, leaves no inbox row, and leaves `seq` unchanged. Until `docs/schema-v2.sql` drops the columns, this section is the contract (§0 item 1: the ADR wins).

## 9. Changelog

### 2026-11-26 — row-level entity_id removed (Doug)

Spec only. §3.3, §3.9, §4.1, §4.3, and §8.15. Postings and ledger accounts have no `entity_id`. INV-001 is per currency. The same-entity rule is leaf-only. The inbox apply checks `target_entity_id` against the singleton `entity.id` and fails closed with `EntityMismatch`. The schema change itself is carried separately (see the PR that made this change).

### 2026-11-26 — six inconsistencies flagged on PR 26

Spec only. `docs/schema-v2.sql` is unchanged. Schema v2 invariants are unchanged: append-only journal, `commits` row written last, deferred foreign keys, `commits_seal`, and caller-supplied `expected_revision` checked inside `BEGIN IMMEDIATE`.

1. **§0 matches §8.2.** Authority item 1 no longer says §8 may only tighten an ADR. Import, outbox/inbox, and `doctor` are named as kernel operations, and the amended ADR 0007 is the same scope.
2. **§1 is the widened kernel.** The boundary prose lists those operations and retires the narrow reading (journal, invariants, and the commit validator only).
3. **No empty commit on a pure retry.** §2.2: an identical re-import or an equal-payload inbox retry that stages nothing rolls back, inserts no `commits` row, does not bump `seq`, and returns `IdempotentNoop` with the original `commit_id` and `seq`.
4. **One freeze location.** §3.13 and §8.13: the active `kernel_state` / `freeze` row in the entity database, written in its own commit, checked under `BEGIN IMMEDIATE` before any write. Doctor reports it. `clear_write_freeze` clears it.
5. **`CurrencyMismatch` is class `user` everywhere.** §4.1, §4.3, §6, and §8.11. It is a validation rejection, including on `import_observations`. It is not class `import` and not a bugcheck.
6. **Inbox payload conflict.** §3.9 and §8.14: the same idempotency key with a different payload hash, including a different amount, is `IdempotencyConflict`. The kernel rejects, writes nothing, and does not overwrite. The hash is SHA-256 over the five canonical payload fields.

`docs/adr/0007-layering.md` has a 2026-11-26 clarification so it does not keep the withdrawn "writes resume at the next clean doctor" sentence or treat every import and inbox apply as a commit that always seals. No other document was edited.
