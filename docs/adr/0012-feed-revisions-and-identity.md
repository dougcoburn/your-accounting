# ADR 0012: Feed identity, observation revisions, and provisional transactions

Status: **Proposed** (2026-11-28). Doug decided the U23 behavior on PR #81. This note records the schema and the follow-up that the behavior needs. It is not accepted. The code in this revision implements the revision table and the identity below so the behavior can be reviewed. It does not post provisional journal entries.

Amends, once accepted: [ADR 0007](0007-layering.md) (natural key and `ImportConflict` on a changed hash) and INV-014 as applied to a later pull of the same feed. Does not amend ADR 0002 for kernel events: observations stay append-only. A later pull appends a row; it does not update the first snapshot.

## Context

Doug's answers on PR #81:

1. Every imported row becomes an observation, including `pending`. An unmatched pending observation should create a provisional transaction that is visible immediately. Later pulls update any changed attribute and keep history.
2. A changed row (same feed id, different values) updates attributes and records history. Void an observation only when a later pull for the same account covers at least ±7 days around the observation's date and the row is absent. A narrower export never voids. Voids stay import-context tombstones (commit kind `import`, no kernel reversal).
3. A Monarch category is a hint. This unit creates no chart accounts from it and does not auto-reconcile a Monarch-reviewed row.
4. The app entry is `add_observations(account, observations[])`. The importer resolves the account first. If it cannot resolve exactly one usable account, the whole import fails and asks the user to map the account or create one. The adapter never creates accounts.
5. With a feed id, identity is that id plus the account. The raw-text hash only detects change. Hints update when the raw row changes, and each raw payload is kept so matching can be re-run. Id-less CSV matching is a follow-up.

## Decision proposed

### Identity

For a source that has a stable id (Monarch `Id`):

`identity = (feed_id, external_id, financial_account_id)`

The same Monarch id on two accounts is two observations. `row_hash` is not part of the identity. It is SHA-256 of the raw column text. The same identity and the same hash is a no-op (§2.2). A different hash is a new revision, not `ImportConflict`.

`IdLessStrategy` is the seam for a CSV or statement with no stable id. It does not match in this unit. The follow-up unit classifies each row as confident, possible-for-review, or new, and uses the statement balance to catch duplicate and missing rows. That unit plugs in behind `IdentityStrategy`. U23 does not implement it.

### Revisions

`observations` stays the first accepted snapshot and stays append-only (no UPDATE, no DELETE). Each later pull of the same identity inserts `observation_revisions` with the full new attributes, including `payload_json` (raw columns and hints). `observation_current` is the latest revision, or the first snapshot when there is none. INV-007's link check reads `observation_current`.

A voided identity is not revised and is not inserted again. The tombstone does not free the key, so a row that reappears after a void stays voided. Restoring one would be a new operation. That limit is part of this proposal.

`ImportConflict` remains the code for a unique-key race the pre-check did not see. It is no longer the result of a changed hash.

### Voids

The adapter, not the kernel, decides absence. For each account in the pull, coverage is the min and max occurred date of that account's rows. An existing live observation on that feed and account is voided with reason `other` when its id is absent and the coverage includes `[occurred_date - 7 days, occurred_date + 7 days]`. The void is `void_observation`: an `observation_voids` row, commit kind `import`, no kernel reversal. Reason `other` is the existing closed set. A dedicated reason code is not added here.

### What this unit does not do

**Provisional transactions are not posted.** A pending observation is stored and is visible as an unreviewed observation (`observation_review_state`). It does not append an `accounting_events` row. Posting a provisional journal entry would touch the kernel: events are append-only, and removing one later is a reversal, which is a one-way door. Proposed follow-up, not decided here:

- An unmatched pending observation (no live `posting_observations` row) gets a provisional transaction the worklist can show immediately.
- "Unmatched" means no live link to a posting.
- The provisional row is not a reconciled event and not a settlement. Accepting it later is an interpretation (`append_event` plus `observation_links`), not an in-place edit.
- Where that provisional row lives (a context table versus a kernel event with a new flag) is the open schema question. This proposal does not add that table and does not add a kernel flag.

Categories are hints on `payload_json` only. No income or expense ledger accounts are created from Monarch names. `Reviewed` stays `reviewed_hint` with `reviewed_authoritative: false`.

## Alternatives

- **Update `observations` in place and keep history in a side table.** Rejected for this proposal. ADR 0002's append-only trigger would have to allow attribute updates. The first snapshot would no longer be the row itself.
- **Keep `ImportConflict` and report column names.** That was the previous U23 default. Doug rejected it: a later pull updates attributes and keeps history.
- **Post a balanced kernel event for every pending row now.** Rejected until the provisional-transaction shape above is accepted. A posted event is hard to undo.

## Consequences

- `docs/schema-v2.sql` in this revision adds `observation_revisions` and `observation_current`, and widens the observation unique key to `(feed_id, external_id, financial_account_id)`. Smoke counts 24 commit-scoped tables.
- KERNEL_INTERFACE_SPEC §3.6 describes the revision behavior so the spec matches the code under review. Acceptance of this ADR is what makes that text normative.
- Id-less matching, statement-balance reconciliation, and provisional transactions are a follow-up unit. They are not U23.
