# ADR 0012: Feed identity, observation revisions, and provisional transactions

Status: **Accepted** (2026-11-27). Doug approved the schema shape by merging #84: `observations.financial_account_id` is nullable, the `cross_check_accounts` table exists, and exactly one of `financial_account_id` and `cross_check_account_id` is set. Doug decided the U23 behavior on PR #81, and on 2026-11-28 decided where a provisional transaction lives and what happens when a voided row reappears. The provisional event (U41) and the explicit restore (U42) are specified below and are not built. U23 does not post provisional journal entries.

Amends [ADR 0007](0007-layering.md) (natural key and `ImportConflict` on a changed hash) and INV-014 as applied to a later pull of the same feed. Does not amend ADR 0002. Observations stay append-only: a later pull appends a row and does not update the first snapshot. A provisional event that changes or is voided also appends a reversal and a replacement. It does not update a sealed event in place.

## Context

Doug's answers on PR #81:

1. Every imported row becomes an observation, including `pending`. An unmatched pending observation should create a provisional transaction that is visible immediately. Later pulls update any changed attribute and keep history.
2. A changed row (same feed id, different values) updates attributes and records history. Void an observation only when a later pull for the same account covers at least ±7 days around the observation's date and the row is absent. A narrower export never voids. Voids stay import-context tombstones (commit kind `import`, no kernel reversal).
3. A Monarch category is a hint. This unit creates no chart accounts from it and does not auto-reconcile a Monarch-reviewed row.
4. The app entry is `add_observations(account, observations[])`. The importer resolves the account first. If it cannot resolve exactly one usable account, the whole import fails and asks the user to map the account or create one. The adapter never creates accounts.
5. With a feed id, identity is that id plus the account. The raw-text hash only detects change. Hints update when the raw row changes, and each raw payload is kept so matching can be re-run. Id-less CSV matching is a follow-up. Doug's expected cadence for that follow-up is below: weekly bank CSV or QFX/OFX, then a month-end statement. U23 does not implement it.

## Decision

### Identity

For a source that has a stable id (Monarch `Id`):

`identity = (feed_id, external_id, financial_account_id)`

The same Monarch id on two accounts is two observations. `row_hash` is not part of the identity. It is SHA-256 of the raw column text. The same identity and the same hash is a no-op (§2.2). A different hash is a new revision, not `ImportConflict`.

A payroll cross-check row has no financial account. Its identity is `(feed_id, external_id, cross_check_account_id)`. The two identities are separate partial unique indexes, because a table UNIQUE treats NULL as distinct and would allow duplicate payroll ids.

### Payroll cross-check

Doug decided this on 2026-11-28 after the real October export. Monarch has a net-zero payroll tracking account with no last 4. Each paycheck is one positive row (gross plus employer benefits minus net) and negative rows for taxes, 401(k), employer benefits, employer HSA, and employee deductions. The paystub is the source of truth (U28). These rows are observations used only to cross-check that paystub group. They never post on their own.

- The binding is explicit and configured by Monarch account name, because there is no last 4. The name is import configuration, not a schema column.
- With that mapping, the adapter binds the name to the non-financial role `payroll` and stores the rows. It does not create a financial account. It does insert the one `cross_check_accounts` row for that role, inside the import transaction.
- Without the mapping, a name-only account fails the whole import with the existing map-or-create message. No account text is in the error.
- A cell that has a last 4 is resolved as a financial account. The payroll name is not consulted.
- Comparing each stored row to the matching paystub line, and flagging a difference such as a tax line off by a stale Social Security amount while the account still nets to zero, is U28. This unit stores the rows and does not compare them.

The schema for the role is in `docs/schema-v2.sql`. Doug approved that shape by merging #84. The comparison is [U28](../MONTH1_IMPLEMENTATION_PLAN.md).

`IdLessStrategy` is the seam for a source with no stable id. Calling it does not match and does not write. The follow-up unit plugs in behind `IdentityStrategy`. U23 does not implement that unit. Its contract is the next section.

### Follow-up: weekly imports and the month-end statement

Spec note only. Not implemented in U23. No new table, no fuzzy matcher, and no statement importer in this revision.

Doug's expected cadence (2026-11-28):

- Each week, import either a bank CSV for a time range or a Quicken QFX/OFX download.
- At month end, import the statement. That import catches and fixes any duplicates the weekly imports left.

Rules for the follow-up unit:

- A QFX/OFX FITID is a feed-provided id. Identity is FITID + account, the same strategy as Monarch `Id` + account: `(feed_id, FITID, financial_account_id)` through `FeedIdStrategy`.
- A time-range CSV with no id uses fuzzy matching against earlier observations, expected transactions, and ledger entries. Each row is one outcome: a confident match, a possible match for review, or new.
- Overlap between weekly import windows must never create a second observation for the same movement.
- The statement is the reconciliation authority through its start and end balances. Those balances are what show a duplicate or a missing row, and the statement import is what fixes them.
- A row must not appear on two statements.
- Raw rows stay stored, on the first snapshot and on each revision, so matching can be re-run.

Still open, and not decided by this note:

- How a confident match is scored, and what a possible match shows the user.
- How the statement import fixes a duplicate that is not a voided row. A voided row that comes back the same stays voided. Restore is the explicit U42 operation, not an automatic un-void.
- Where statement membership is stored so "a row must not appear on two statements" can be checked. This note adds no table.

### Revisions

`observations` stays the first accepted snapshot and stays append-only (no UPDATE, no DELETE). Each later pull of the same identity inserts `observation_revisions` with the full new attributes, including `payload_json` (raw columns and hints). `observation_current` is the latest revision, or the first snapshot when there is none. INV-007's link check reads `observation_current`.

A voided identity is not revised and is not inserted again. Doug decided the reappearance rule in the section below. U23 already implements the stay-voided half of it.

`ImportConflict` remains the code for a unique-key race the pre-check did not see. It is no longer the result of a changed hash.

### Voids

The adapter, not the kernel, decides absence. For each account in the pull, coverage is the min and max occurred date of that account's rows. An existing live observation on that feed and account is voided with reason `other` when its id is absent and the coverage includes `[occurred_date - 7 days, occurred_date + 7 days]`. The void is `void_observation`: an `observation_voids` row, commit kind `import`, no kernel reversal. Reason `other` is the existing closed set. A dedicated reason code is not added here.

### Provisional transactions (decided 2026-11-28)

Doug decided the shape. U23 does not implement it. It is [U41](../MONTH1_IMPLEMENTATION_PLAN.md).

- A provisional transaction is a kernel event with a provisional flag. It is not a context-only row.
- Balances include provisional and pending entries, even though those entries can change or be voided.
- A recipe commit of a transaction group must not use a provisional or pending entry. Only a posted entry is eligible.
- "Unmatched" means no live `posting_observations` row. An unmatched pending observation creates the provisional event. It is visible immediately.
- Accepting it later, once it is no longer pending, is an interpretation (`append_event` plus `observation_links`), not an in-place edit of the sealed event.

U23 stores the pending observation and leaves it unreviewed, with no postings. That is the visible observation. It is not the provisional event.

U41 is its own unit because the flag is not a small adapter change:

- INV-001 requires the event to balance per currency. An unmatched pending observation names one financial account and an amount. U23 creates no ledger account from the Monarch category hint, so the other leg has no account yet. Choosing that offset account is part of U41.
- ADR 0002 allows one update of an event: `superseded_by`, set once, to the reversing event, inside its commit. A provisional entry that changes or is voided appends a reversal and, when it continues, a replacement. It does not flip a flag on a sealed row.
- `account_balances` and `trial_balance` already sum every posting. Including these entries follows from posting them. Keeping them out of a recipe commit group is a new rule on the settlement and recipe commit path (U26, U27).
- ADR 0011 keeps import from posting kernel events. U23 does not cross that line.

U41 acceptance:

- An unmatched pending observation creates a balanced kernel event with the provisional flag set, in the import commit.
- `account_balances` and `trial_balance` include that event's postings, and they include a pending entry the same way.
- A recipe commit of a transaction group that contains a provisional or pending entry is rejected. A group made only of posted entries commits.
- A later change to the observation, or a void of it, changes or voids the provisional event by appending. The sealed event row is not rewritten in place.
- One predicate defines a posted entry: the event is not provisional, and the observation is not pending.

### Voided rows that reappear (decided 2026-11-28)

Doug decided:

- A voided row that reappears and looks the same as what was voided stays voided. There is no auto-restore.
- Restoring is an explicit action.
- When a statement's ending balance does not reconcile, the reconciliation flow surfaces voided observations as one of the candidate sources for the missing observations.

U23 already does the first bullet. A voided identity is left as it is: no second insert and no revision. The tombstone does not free the key.

The explicit restore, and the reconciliation candidate list, need statement import. They are [U42](../MONTH1_IMPLEMENTATION_PLAN.md), not this unit.

U42 acceptance:

- Re-importing a voided identity with the same attributes leaves the tombstone and writes no observation and no revision.
- An explicit restore makes that observation live again. Import does not call it.
- When a statement ending balance does not agree, the reconciliation result includes the voided observations in that account's window among the candidates for the missing rows.

### What this unit does not do

Categories are hints on `payload_json` only. No income or expense ledger accounts are created from Monarch names. `Reviewed` is the status word `Reviewed` (hint true) or blank (hint false). `true` and `false` stay accepted aliases. `reviewed_authoritative` stays false. U23 does not post the provisional event (U41), does not restore a void (U42), and does not post a payroll cross-check row.

## Alternatives

- **Update `observations` in place and keep history in a side table.** Rejected for this proposal. ADR 0002's append-only trigger would have to allow attribute updates. The first snapshot would no longer be the row itself.
- **Keep `ImportConflict` and report column names.** That was the previous U23 default. Doug rejected it: a later pull updates attributes and keeps history.
- **Post the provisional kernel event inside U23.** Doug accepted the shape (a kernel event with a provisional flag). U23 still does not post it. The offset leg, the append-only change and void, and the recipe-group exclusion are U41.

## Consequences

- `docs/schema-v2.sql` in this revision adds `observation_revisions` and `observation_current`, and widens the observation unique key to `(feed_id, external_id, financial_account_id)`. Smoke counts 24 commit-scoped tables.
- KERNEL_INTERFACE_SPEC §3.6 describes the revision behavior. This ADR is accepted, so that text is normative.
- Id-less matching and the weekly CSV / QFX cadence are a follow-up. They are not U23. The cadence rules above are the spec for that unit.
- Provisional events are U41. Explicit restore, and surfacing voided observations when a statement ending balance does not reconcile, are U42. Both are decided here and not built.
- A configured payroll-tracking name binds to `cross_check_accounts` role `payroll`. `observations.financial_account_id` is nullable, `cross_check_account_id` is the other binding, and exactly one is set. Those rows are stored and are not posted. The paystub comparison is U28. Doug approved that schema shape by merging #84.
