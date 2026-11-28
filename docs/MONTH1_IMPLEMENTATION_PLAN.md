# your-accounting: month-1 implementation plan (spec, for review)

This plan is the month-1 spec for now. It is still open for Doug's review and merge. **U17 already landed in #31** before this plan was approved. Further implementation (U18 and later) still waits on approval of this plan.

Drafted 2026-11-22 against `main` at d72968c (MAP, GLOSSARY, SPEC v0.2, ADR 0001–0008, `accounting-core/`) and the local design copies. Refreshed 2026-11-26 against `main` at db059cbeacc7d04d169395e6c9f7344dbb900c4d, after these pull requests merged:

- #19 — U15, kernel schema v2 (fresh migration 1) and the SPEC v2 delta, plus the doc-drift fixes noted below
- #22 — U03, seeded synthetic October 2025 Monarch fixture
- #24 — employer name replaced with the generic placeholder "Acme Corp", plus the denylist guard
- #25 — U16, [`KERNEL_INTERFACE_SPEC.md`](KERNEL_INTERFACE_SPEC.md)
- #26 — ADR 0007 amended to match the kernel interface spec; kernel-spec question Q6 decided
- #27 — six kernel-spec inconsistencies from #26 resolved

Refreshed again 2026-11-26 against `main` at 523a3c622401c9341f80dd1356155721153b36bd, after these further pull requests merged, and to record Doug's answers to the plan's open questions:

- #31 — U17, typed ledger accounts and parties
- #32 — ADR 0004 rewrite: no row-level `entity_id`; recipe groups via `recipe_instance_id`; user-scoped registry
- #33 — ADR 0009, single-currency accounts and FX Clearing (post-month-1; no month-1 unit depends on it)
- #34 — kernel write path attaches `InvalidShape` token `recipe_party`

**Destination (MAP):** the October 2025 Monarch CSV goes from imported observations to a posted, balanced ledger. Every observation is explained by a recipe and ends reconciled or voided, or stays open on the worklist.
**Out of scope:** issues #6–#17 (deferred), MCP and agent auth, statement feeds, multi-month work, polished reports.

## Progress as of 2026-11-26

| Status | Units |
|---|---|
| Done | U03 (#22, employer placeholder cleaned in #24), U15 (#19), U16 (#25, amended by #26 and #27), U17 (#31) |
| Partial | U01 — `scripts/check-denylist.sh` landed in #24; the CI workflow (fmt, clippy, test, synthetic-marker / CSV placement guard) and `.gitignore` entries for `local.config.json` and real CSV paths are still open. U04 — the real October CSV is in place locally; October paystubs and bank statements will be placed in `/workspace/ya-private-data` (question 1, decided). Posted dates will come from the Monarch Date column (question 8, decided). |
| In review, not merged | U02 — product brief in [#18](https://github.com/dougcoburn/your-accounting/pull/18). Prototypes A, B, and C are open as throwaway alternatives in [#21](https://github.com/dougcoburn/your-accounting/pull/21), [#20](https://github.com/dougcoburn/your-accounting/pull/20), and [#23](https://github.com/dougcoburn/your-accounting/pull/23), covering much of the U05–U10 screen set. Short feedback checks (U11 / U12) run as each screen lands. U13 is a living record of that feedback, not a freeze before build. This plan is the month-1 spec. |
| Not started | U18–U42. The kernel critical path continues at **U18**. U41 and U42 are the provisional-event and explicit-restore follow-ups decided on PR #81. |

This plan is the month-1 spec pending Doug's merge. U17 already landed in #31. Further implementation still waits on approval of this plan.

## Design decisions now on main (2026-11-26)

Cross-references only; the unit list below is not rewritten to match every schema line.

- **No row-level `entity_id`** (#31, #32). The entity database has one identity record; rows do not carry `entity_id`. See [ADR 0004](adr/0004-entity-isolation-cross-entity-recipes.md).
- **Recipe groups** are joined by `recipe_instance_id`. Cross-entity halves are application-orchestrated; later units (U25, U29) follow the rewritten ADR 0004.
- **User-scoped registry.** The application keeps a per-login registry of entity databases (ADR 0004).
- **FX Clearing** is specified in [ADR 0009](adr/0009-single-currency-accounts-fx-clearing.md) and is **post-month-1**. No month-1 unit depends on it.

## Ground rules

- **Real data stays out of agents and repos.** No real rows in any repo, PR, issue, CI log, commit message, feedback log or agent prompt. Agents get structure only (column names, account types, last-4). Sharing the column header row alone (no data rows) with agents as structure is OK (question 1, decided 2026-11-26).
- **Where to view.** Synthetic transactions are viewed in the box desktop browser. Real transactions are viewed on Doug's Mac, with the real CSV kept there.
- **Where the real data lives.** The real CSV is at `/workspace/ya-private-data/october-2025-monarch.csv` (mode 600, outside every git working tree). The app reads it only at runtime, through `YA_MONARCH_CSV` (or a gitignored `local.config.json`), and never copies it, caches it, or logs rows. Team bots never read that folder. Only Doug's browser session (or the main bot acting for Doug) opens it.
- **Fixtures are synthetic.** Every committed fixture is generated, Monarch-shaped and carries a `# synthetic` marker. CI rejects any CSV or JSON under the repo that lacks the marker.
- **Denylist.** `scripts/check-denylist.sh` must pass on every change. It scans tracked text for hashed employer and personal-name tokens. The forbidden terms themselves are not in the repository. See [`CONTRIBUTING.md`](../CONTRIBUTING.md).
- **Process.** One writer at a time per area (kernel crate = BE, recipes/adapter/CLI = FS, prototype/UI = FE). Agents open PRs and Doug merges. The QA gate (all tests plus `doctor` clean on fixtures) is the source of truth. No swarm chatter: units hand off through PRs and the unit table.
- **Feedback log.** Doug's prototype reactions go to [`product/feedback-log.md`](product/feedback-log.md) as decisions only ("prefers X over Y because Z"), never amounts or merchants. That file is proposed in #18 and is not on `main` yet.

## Real CSV shape (aggregates only, computed locally 2026-11-22)

Counts and column names only. No merchants, amounts, account numbers, or row-level data.

- **Columns:** Date, Merchant, Category, Account, Original Statement, Notes, Amount, Tags, Owner, Reviewed, Reviewed At, Reviewed By, Pending, Id.
- **Rows and coverage:** 199 data rows (219 physical lines; some fields span lines). Every date falls in Oct 2025.
- **Accounts, categories, owner:** 12 accounts and 44 Monarch categories. The Owner column has a single value. The chart of accounts is seeded from those Monarch categories; Doug adds more as needed (question 4, decided 2026-11-26).
- **Amount sign:** 158 negative rows (outflow) and 41 positive (inflow), none zero. The sign is the direction.
- **Other columns:** Pending is false and Reviewed is set on every row. Carry Reviewed as a strong hint (high confidence the Monarch categorization is correct) but not as authoritative (question 9, decided 2026-11-26). 197 rows carry e-* tags (7 distinct tag values). 6 rows have notes. About 29 rows sit in transfer or payment categories.
- **Id:** `Id` is present and unique on every row, so the adapter uses it as the natural external id (ADR 0007's derived id is only a fallback).
- **Dates:** there is one Date column, imported by Monarch from the connected bank accounts. Those Date values are used as posted dates; Doug supplies no separate posted-date source (question 8, decided 2026-11-26). With a single column, occurred and posted are the same value this month.
- **Sizing:** the synthetic generator (U03, now landed) mirrors these proportions with invented values. The committed fixture is documented in [`fixtures/monarch/README.md`](../fixtures/monarch/README.md).

## Existing code: keep vs rewrite (`accounting-core`; U17 now on main as of 2026-11-26)

The original 18 kernel and recipe tests are still present. U03 added the generator and fixture checks. U17 (#31) and the `recipe_party` token (#34) landed after the first refresh.

| Area | Verdict | Why |
|---|---|---|
| Cargo workspace (kernel / recipes / cli crates), rusqlite | Keep | Layering matches [ADR 0007](adr/0007-layering.md) |
| `Money`, `Currency`, typed `KernelError`, DbC style in [`KERNEL_INTERFACE_SPEC.md`](KERNEL_INTERFACE_SPEC.md) | Keep | Matches the money decision. Bugcheck policy and the 4-way triage protocol landed in U16 (#25, amended by #26 and #27) |
| `begin_transaction(expected_revision)` boundary | Keep | Basis for atomic commits and AT-032 |
| Balance seal trigger per (entity, currency), posting immutability triggers | Keep, adapt | INV-001 |
| `settlement_group_postings` UNIQUE(posting) | Keep | Settlement exclusivity ([ADR 0006](adr/0006-recipe-owned-suggestion-only-matching.md)) |
| `ledger_accounts.element` (revenue), `role`=contra | Rewrite | [ADR 0008](adr/0008-pure-double-entry-categories-are-ledger-accounts.md): `account_type` in {asset, liability, equity, income, expense}, plus `kind`, `parent_id` and leaf-only postings. No contra type. Schema is specified in [`schema-v2.sql`](schema-v2.sql); typed accounts and parties landed in U17 (#31). |
| `entities.visibility` (transparent/opaque) | Remove | [ADR 0003](adr/0003-parties-and-viewer-relative-visibility.md): Party plus access grants |
| `journal_entries.status` UPDATE, `replaces_entry_id` 1:1 lifecycle triggers | Rewrite | ADR 0002 amendment: append-only `accounting_events` with `recorded_at`, `reverses_id`, and `superseded_by` set once in the reversing commit. Multi-event commits |
| Settlement group seal (status update) | Rewrite | Group state is derived from append-only group commits |
| Per-prefix ids (`je_1`) | Rewrite | [ADR 0004](adr/0004-entity-isolation-cross-entity-recipes.md) needs globally unique ids (ULID). Clock and id source injectable for deterministic tests. The #32 rewrite of ADR 0004 also drops row-level `entity_id` and uses `recipe_instance_id` plus a user-scoped registry. |
| `split_purchase_across_entities` (one entry spanning entities) | Rewrite | ADR 0004 (#32): application-orchestrated halves in per-entity DBs, joined by `recipe_instance_id` |
| Payroll via committed clearing (acceptance_suite) | Rewrite | Architect answer: the HSA expected slot is its own draft group |
| CLI `main.rs` (demo printout) | Replace | Needs real commands (see U36) |
| Existing tests | Port | Become synthetic acceptance tests under the new rules. Delete each old test only once its replacement is green |
| Docs drift | Done in U15 (#19) | [`schema-v0.2.sql`](schema-v0.2.sql) is marked superseded (it still contains `visibility`, `opaque_entity_id` and mutable `state`/`revision` as the historical v0.2 text). [MAP](design/MAP.md) line 50 and the [GLOSSARY](../GLOSSARY.md) "Transaction date vs. posting date" entry now say reconciliation uses the occurred date, which matches the 2026-11-22 decision. |

No real database exists yet. The v2 schema landed as a fresh migration 1 in #19. The SPEC's "upgraded DB" test starts at v2.

## Phases

0. **Guardrails** (days 1-2): CI, data guard, synthetic October fixture, product brief. Real CSV already supplied locally. U03 and U15 are done; U01 and U02 are not finished.
1. **Prototype loop** (runs first, in parallel with kernel phase 2): a throwaway TypeScript single-page app on 127.0.0.1 with an in-browser toy ledger. It loads the synthetic fixture by default (viewed in the box desktop browser) and Doug's real CSV from `YA_MONARCH_CSV` (viewed on Doug's Mac, with the CSV kept there). A short feedback check runs as each screen lands. This plan is the month-1 spec; U13 is a living record of prototype feedback, not a gate before build. Three alternative prototypes (A, B, C) are already in review; pick or mix from those rather than rebuilding U05–U10 from scratch unless review says otherwise.
2. **Kernel v2** (UI-independent, so it starts now): typed accounts, append-only events, commit validator, `doctor`. Docs (U15, U16) and U17 (#31) are done. Next code is U18, after this plan is approved.
3. **Import**: observation store and the Rust Monarch adapter (the Monarch Date is the posted date; natural key; void).
4. **Recipes and settlement**: recipe contract, append-only groups, recipe batches, cross-entity recipe halves and the recipe check, match suggestions. Follow the rewritten ADR 0004 already on main.
5. **Reconciliation**: balance assertions, occurred-date balance, in-flight explanation.
6. **Worklist and corrections**: derived review state, register/audit views, correct/void commands.
7. **CLI/UI and month close**: CLI, a real local web UI over the Rust app (U37), golden end-to-end gate, real October dry run on the box.

## Work units

Size: S is about 1 day, M is 2-4 days. "Doug" units cannot be delegated.

| ID | Title | Owner | Depends on | Acceptance check | Size | Status |
|---|---|---|---|---|---|---|
| U01 | CI (fmt, clippy, test) + data guard (synthetic marker check, block CSVs outside `fixtures/`, block >4-digit account numbers, `.gitignore`) | DevOps | - | CI red on a planted unmarked CSV, green on main | S | Partial (#24 denylist only) |
| U02 | Month-1 product brief: screens, done criteria, feedback-log template | PM | - | [`product/month1-brief.md`](product/month1-brief.md) PR; Doug approves | S | In review (#18) |
| U03 | Synthetic October generator (seeded; ~200 rows, 12 accounts, ~44 categories, ~80/20 out/in, e-* tags on almost all rows, unique Id, same 14 columns) covering card buys, card payment, paycheck, HSA missing side, Dragonfly e-tagged buy, Acme Corp partial reimbursement, refund, perk credit, duplicate row, in-flight items | Data Sci | U01 | Deterministic output; every scenario in a coverage table; passes the data guard | M | Done (#22, #24) |
| U04 | Oct 2025 Monarch CSV in `/workspace/ya-private-data/` (**done 2026-11-22**); October paystubs and bank statements will be placed there (Captain copies them; team bots never read the folder). Column header row alone may be shared with agents as structure. Posted dates come from the Monarch Date column (no extra file). | Doug | - | Files present locally; nothing in git | S | Partial (CSV in; paystubs and statements forthcoming) |
| U05 | Prototype shell: localhost server, CSV loader (fixture or `YA_MONARCH_CSV`), TS Monarch adapter, read-only register | FE | U01, U03 | Loads fixture; real path from env/gitignored config only; no rows in console or server logs; binds 127.0.0.1 | M | In review (prototypes A/B/C: #21, #20, #23) |
| U06 | Worklist view: origin (observed/expected), state (unreviewed/interpreted/reconciled), occurred vs posted dates, filters | FE | U05 | All fixture rows visible; state counts shown | S | In review (see U05) |
| U07 | Recipe picker: choose recipe per line, debit/credit preview, category tree per entity, seeded from imported Monarch categories (Doug adds more as needed) | FS | U05 | Every fixture scenario can pick a recipe and shows a balanced preview | M | In review (see U05) |
| U08 | Settlement grouping: per-recipe suggestions, confirm or choose participants, residual, expected slot | FS | U07 | Transfer, paycheck/HSA and reimbursement scenarios groupable | M | In review (see U05) |
| U09 | Reconciliation screen: occurred-date balance, in-flight list both directions, statement ending, difference | FE | U06 | Fixture in-flight items explain the gap exactly | S | In review (see U05) |
| U10 | Corrections screen: edit shows reversal + replacement, register hides the pair, audit toggle shows lineage | FE | U07 | Correction-of-correction (AT-033 shape) displays right | S | In review (see U05) |
| U11 | Short feedback checks as register, worklist, and recipe-picker screens land, with Doug | PM | U04, U06, U07 | Feedback log PR, decisions only | S | Not started |
| U12 | Short feedback checks as grouping, reconciliation, and corrections screens land | PM | U08, U09, U10 | Feedback log PR, decisions only | S | Not started |
| U13 | Living record of prototype feedback (not a gate before build); fold decisions into GLOSSARY/MAP/ADRs as they land. This plan remains the month-1 spec. | PM + Architect | U11, U12 | [`design/MONTH1-SPEC.md`](design/MONTH1-SPEC.md) kept current as feedback lands; later units do not wait on a freeze | S | Not started |
| U14 | Prototype privacy audit: no real data in repo, build output, logs or browser storage; no network egress | QA | U05 | Checklist signed in PR | S | Not started |
| U15 | Kernel schema v2 doc: apply ADR 0002 amend/0003/0004/0008 deltas; fix the doc drift above | Architect | - | [`schema-v2.sql`](schema-v2.sql) + SPEC delta PR; Principal approves | M | Done (#19) |
| U16 | Kernel interface update: DbC table, typed errors, bugcheck policy, triage protocol, admission test | Principal | U15 | [`KERNEL_INTERFACE_SPEC.md`](KERNEL_INTERFACE_SPEC.md) PR | S | Done (#25, #26, #27) |
| U17 | Typed ledger accounts + parties: `account_type`, `kind`, `parent_id`, leaf trigger (INV-003), drop visibility/contra | BE | U15 | AT-002 and leaf/same-entity tests green | M | Done (#31) |
| U18 | Append-only events: `accounting_events`, `recorded_at`, `reverses_id`, set-once `superseded_by` trigger, partial index, UPDATE/DELETE raise, ULID + injectable clock | BE | U17 | Trigger tests: any other update rejected | M | Not started |
| U19 | Commit validator: atomic multi-event commit, INV-001 per entity/currency, expected revision, closed-month check | Principal | U18 | AT-013, AT-032 green; no partial state on failure | M | Not started |
| U20 | Kernel acceptance: AT-033, AT-034 + property test (balance = sum of postings, debits = credits) | QA | U19 | Tests in CI | M | Not started |
| U21 | `doctor` v1: partition balance, `superseded_by` vs `reverses_id`, FK/integrity, nonzero exit | BE | U18 | Planted drift detected | S | Not started |
| U22 | Observation store: natural key (feed + Monarch `Id`), occurred date (posted date optional, nullable), row hash, idempotent re-import, void tombstone | BE | U18 | Re-import adds 0 rows; a void tombstones the observation and ends its live links without reversing the kernel event | M | Not started |
| U23 | Rust Monarch adapter: CSV to observations via `add_observations(account, rows)`. Identity is feed id + account. Pending rows are stored. A changed hash appends a revision (proposed [ADR 0012](adr/0012-feed-revisions-and-identity.md)). Category is a hint only. A financial account must already exist. A trailing parenthetical last 4 is accepted, and `Reviewed` is a status word. A configured name with no last 4 binds to the payroll cross-check role and is stored, not posted; the paystub comparison is U28. Void only when the pull covers ±7 days and the id is absent. A voided row that reappears the same stays voided. Not built here: weekly bank CSV or QFX/OFX then a month-end statement; provisional events (U41); explicit restore and voided-observation candidates (U42) | FS | U22, U03 | Fixture imports with a 1:1 row count | M | Not started |
| U24 | Import tests: duplicates, corroboration, minor units, malformed rows | QA | U23 | Tests in CI | S | Not started |
| U25 | Recipe contract: ChangeSet shape, match-hint declaration, versioning (AT-012), `recipe_instance_id` | Architect | U16 | ADR/SPEC PR (follow living U13 feedback as it exists; not gated on a U13 freeze) | S | Not started |
| U26 | Append-only settlement groups: exclusivity, zero residual per group/entity/currency, drafts never post | BE | U19 | AT-004, AT-030, INV-006 tests | M | Not started |
| U27 | Recipes A: purchase→category, refund/contra (linked or standalone, cap), perk credit, own-account transfer | FS | U25, U26 | AT-003, AT-017, AT-018 green | M | Not started |
| U28 | Recipes B: paycheck gross-to-net from manual paystub, pre-tax/Roth separate accounts, HSA expected slot as its own draft group. Compare each Monarch payroll cross-check observation to the matching paystub group line, flag a difference (a tax line off by a stale Social Security amount while the account still nets to zero), and never post those observations | FS | U27 | AT-019/020 (adapted, no committed clearing) green, and a payroll cross-check difference is flagged and not posted | M | Not started |
| U29 | Cross-entity on-behalf + reimbursement claims: application-orchestrated recipe halves (one per entity database, joined by `recipe_instance_id`; identical retry is a no-op), partial settle, forgive (user picks category) | BE | U26, U25 | AT-001, AT-005-007 + ADR 0004 crash/retry/mismatch tests | M | Not started |
| U30 | Match suggestions per recipe: deterministic order, bounded N-sum | BE | U27 | Stable-order tests | S | Not started |
| U31 | Recipe acceptance pass, including cross-entity recipe halves (ADR 0004: application-orchestrated halves joined by `recipe_instance_id`) and the application-level recipe check (counterpart not visible, missing half, orphaned 7 days, duplicate, mismatched counterpart, mismatched amount per currency); per-database `doctor` stays single-database | QA | U28, U29, U30 | All recipe ATs and recipe-check status tests green in CI | M | Not started |
| U32 | Balance assertions + occurred-date reconciliation with in-flight explanation (both directions) | BE | U26 | AT-009/010/014/024 adapted to the occurred date | M | Not started |
| U33 | Reconciliation tests on the fixture's in-flight items | QA | U32 | Tests in CI | S | Not started |
| U34 | Register/worklist read model: derived per-line state, origin, hide superseded pairs, audit lineage | FS | U26 | Register matches this plan and living U13 feedback on the fixture | S | Not started |
| U35 | Correct and void commands: reversal + replacement, regroup, transitive validation | BE | U26, U32 | AT-022, AT-023, AT-033 end to end | M | Not started |
| U36 | CLI: init, import, worklist, apply-recipe, group, assert, reconcile, correct, void, doctor; `--json` | FS | U23, U31, U34, U35 | Scripted fixture month runs end to end | M | Not started |
| U37 | Month-1 UI: port the Doug-approved prototype screens onto the real app (local JSON API). End state is a real local web UI over the Rust app. | FE | U36 | Screens backed by the kernel on the fixture | M | Not started |
| U38 | Local runbook: data dir outside repo, per-entity DB backups local only | DevOps | U36 | Runbook PR; dry run on a clean box | S | Not started |
| U39 | Golden synthetic month gate: every row reconciled, voided or open; trial balance closes; `doctor` clean | QA | U36 | CI job; this is the month-1 gate | M | Not started |
| U40 | Real October dry run on the box; report counts only (reconciled/voided/open) | Doug + PM | U39, U04 | Doug signs off; no rows leave the box | S | Not started |
| U41 | Provisional kernel events (ADR 0012, decided 2026-11-28): a provisional flag on the event. An unmatched pending observation creates a balanced one. Balances include provisional and pending entries. A recipe commit group accepts only posted entries. A later change or void appends; a sealed event is not rewritten | BE | U23, U19, U26 | Unmatched pending row posts a provisional event that the balance views include; a recipe commit group containing one is rejected; a posted-only group commits; a changed or voided observation appends | M | Not started |
| U42 | Explicit observation restore, and reconciliation candidates (ADR 0012, decided 2026-11-28). A voided row that reappears the same stays voided (already U23). Restore is an explicit action import does not call. When a statement ending balance does not reconcile, voided observations in that window are candidate missing rows | BE | U32, U23 | Same-looking re-import stays voided; explicit restore makes it live; a statement mismatch lists those voided observations | M | Not started |

U41 and U42 are follow-ups decided on PR #81. They are in this table. U23 does not implement them: U41 crosses INV-001, ADR 0002, and the recipe commit path, and U42 needs statement import.

**Units by role:** BE 11 (U17, U18, U21, U22, U26, U29, U30, U32, U35, U41, U42); FS 7; QA 6; FE 5; PM 5 (U13 shared with Architect, U40 with Doug); Architect 2; Principal 2; DevOps 2; Data Scientist 1; Doug 1 (+U40). That is 42 in total. BE is the critical path, so the Principal takes the commit validator (U19).

## Parallel lanes

- **Day 1, all at once:** U01 (DevOps), U02 (PM), U15 (Architect), U04 (Doug). U03 starts as soon as U01 lands. U03 and U15 are done; U01 is partial; U02 is in review.
- **Prototype lane (FE then FS):** U05 → U06 / U07 in parallel (different files) → U08, U09, U10, with short feedback checks as each screen lands (U11 / U12). QA runs U14 alongside. U13 records feedback as it lands and is not a freeze gate. In practice, three alternative prototypes were built and are waiting on Doug's review; remaining prototype work should follow that review rather than restart the sequence.
- **Kernel lane (BE, Principal, QA):** U16 (done) → U17 (done, #31) → **U18** → U19 → U20 / U21 / U22. This lane does not depend on prototype feedback. **U18 is the next unit once this plan is approved.**
- **Recipes and later (no U13 freeze gate):** U25 → U26 → U27 → U28, with U29, U30 and U32 running in parallel. FS and BE work in different crates.
- **Close:** U34 / U35 in parallel → U36 → U37 (real local web UI over the Rust app) / U38 / U39 → U40.
- **Gates:** QA owns U20, U24, U31, U33 and U39. Nothing in a later phase merges while its phase gate is red.

## Prototype feedback questions for Doug (asked by the PM; answers logged as decisions)

These are also pre-seeded as FQ-01–FQ-22 in the #18 feedback-log draft.

- **Register / worklist:** One list with state filters, or separate "to do" and "done" tabs? Which columns matter (occurred, posted, account last-4, merchant, amount, suggested category, entity hint)? Should Monarch's category show as a hint or be hidden? Should expected lines sit inline with their source or in their own section? Is sorting by occurred date right?
- **Recipe picker:** Choose the recipe first and then fill in details, or pick a suggested treatment in one click? Should the debit/credit preview always show, or be behind a toggle? How deep should the category tree go per entity? Seed it from imported Monarch categories; Doug adds more as needed (question 4, decided). Is the vocabulary right (recipe, interpreted, on-behalf)?
- **Settlement grouping:** Should suggestions open automatically, or only when you ask? What evidence do you need before confirming (dates, memo, residual)? How should a paycheck's per-line groups look: one card with sub-lines, or separate items? How should an open HSA expected slot nag you?
- **Reconciliation vs statement:** Is "occurred balance → in-flight items → statement ending" readable? Posted dates come from the Monarch Date column (question 8, decided). October bank statements will be placed in the private folder (question 1, decided). Will you type statement endings by hand for October, or skip that this month? Should a mismatch block anything, or just flag it?
- **Corrections / void:** Should an edit look like an edit, with the reversal hidden unless audit is on? Is a void confirm-and-done, or should it ask for a reason? Should you see "changed after close" markers on closed months?
- **Overall:** Which screen would you use daily? What is missing? What should be dropped from month 1? Which of prototypes A, B, and C (or which parts of each) should be kept?

## Questions for Doug

Several of these were also restated in #18. All numbered questions are now decided. Answers below are from 2026-11-26 unless noted.

### Decided

1. **Rest of the local data.** **Decided 2026-11-26:** Doug will pull his October paystubs and bank statements now, and they will be placed in `/workspace/ya-private-data` (Captain copies them; team bots never read that folder). Sharing the column header row alone (no data rows) with agents as structure is OK.
2. **Where to view the prototype.** **Decided 2026-11-26:** synthetic transactions in the box desktop browser; real transactions on Doug's Mac with the CSV kept there.
3. **Month-1 UI end state.** **Decided 2026-11-26:** a real local web UI over the Rust app (U37), not CLI plus the prototype as a reference only. The MAP still keeps a polished UI out of scope.
4. **Chart of accounts seed.** **Decided 2026-11-26:** import Monarch categories to start; Doug adds more as needed.
5. **Rewrite without migration.** ~~OK to replace the kernel schema (status/visibility model) without migrating, since no real DB exists?~~ **Decided by merge of #19:** yes. Schema v2 is a fresh migration 1. No v0.2 data migration.
6. **Feedback cadence.** **Decided 2026-11-26:** a short check as each screen lands, not two fixed sessions.
7. **Month-1 spec.** **Decided 2026-11-26:** this plan (#29) is the month-1 spec for now. U13 becomes a living record of prototype feedback, not a gate before build. No separate spec PR is required before work starts.
8. **Posted dates.** **Decided 2026-11-26:** use the dates Monarch imported from the connected bank accounts as posted dates. Doug supplies nothing extra. The CSV has one Date column; occurred and posted are the same value this month.
9. **Monarch Reviewed column.** **Decided 2026-11-26:** carry it as a strong hint (high confidence it was categorized correctly in Monarch) but not as authoritative or 100% perfect.

The prototype feedback questions above (layout, recipe flow, grouping, reconciliation vs statement endings, corrections/void), and which of prototypes A, B, and C to keep, are still open.
