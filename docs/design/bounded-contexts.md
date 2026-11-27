# Bounded contexts and context map (proposed)

**Status:** Proposed with [ADR 0010](../adr/0010-bounded-contexts-small-agent-context.md), 2026-11-26. Docs only. The normative sources are unchanged: [`KERNEL_INTERFACE_SPEC.md`](../KERNEL_INTERFACE_SPEC.md) ("KIS"), [`SPEC-v2-delta.md`](../SPEC-v2-delta.md) ("delta"), [`schema-v2.sql`](../schema-v2.sql), the ADRs, and [`GLOSSARY.md`](../../GLOSSARY.md). This page proposes how to cut them. It does not restate their rules.

**Principle (Doug, 2026-11-26):** smaller context = smarter agent work. When the glossary terms and independent concepts needed for a change get numerous, split the design with DDD into bounded contexts with small interfaces. An interface should be understandable without its implementation.

## 1. How to read this page

- §2 measures what a contributor loads today for each upcoming unit.
- §3 proposes the contexts, with each one's interface and load list.
- §4 is the context map. §5 covers split now versus later. §6 covers doctor. §7 is the first step. §8 lists Doug's decisions.

Word counts come from `wc -w` on `main` at `24dee26`. Token figures are **estimates**. They use a rough rule of about 1.3 tokens per English word, and more for SQL. They were not measured.

## 2. Context-size audit

### 2.1 Document sizes (`main` at `24dee26`)

| Document | Lines | Words | Note |
|---|---:|---:|---|
| `docs/KERNEL_INTERFACE_SPEC.md` (v2 kernel) | 760 | 13,177 | 8,602 words at #25 (2026-11-25). §3.9 has 922 words, §4.1 913, §4.3 964, and §0 to §1 about 800 words of authority and scope recap. |
| `docs/schema-v2.sql` | 1,030 | 10,188 | 36 tables, 11 views |
| `docs/SPEC.md` (v0.2 base) | 447 | 4,901 | Superseded in part by the delta |
| `docs/adr/0009` FX Clearing | 232 | 4,575 | Larger than ADRs 0001–0003 and 0005–0008 together (3,145 words) |
| `docs/SPEC-v2-delta.md` | 223 | 4,276 | |
| `docs/MONTH1_IMPLEMENTATION_PLAN.md` | 187 | 4,285 | |
| `accounting-core/docs/KERNEL_INTERFACE_SPEC.md` (v1, legacy) | 756 | 4,832 | Same file name as the v2 spec, which is a mis-load risk |
| `GLOSSARY.md` | 204 | 2,947 | 57 entries. 773 words on 2026-11-20, 1,250 before #32, 2,211 after #32, 2,947 after #33. |
| `docs/design/MAP.md` | 106 | 2,664 | |
| `docs/adr/0004` cross-entity | 137 | 2,394 | 271 words before 2026-11-26 |
| `docs/adr/0007` layering | 63 | 1,055 | Three amendments stacked on the decision |
| ADRs 0001, 0002, 0003, 0005, 0006, 0008 | | 285 / 449 / 246 / 232 / 240 / 638 | |

The full normative set is about 48,000 words. That is about 65,000 tokens or more by estimate, before code.

### 2.2 What each unit must load today

This table counts what a contributor has to read to make the change correctly. Unit definitions are from [`MONTH1_IMPLEMENTATION_PLAN.md`](../MONTH1_IMPLEMENTATION_PLAN.md) (merged in #29). "ADR 0009 FX" is the post-month-1 FX unit. The counts are a reviewer's reading, not tool output.

| Unit | Change | Glossary terms | ADRs | KIS §s | Delta §s | Schema tables/views | Today (est., words) | Scoped (est., words) |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| U17 (done, #31) | Typed accounts and parties | ~12 | 4 (0003, 0004, 0005, 0008) | ~6 (§1, §3.1, §3.3, §4.1, §4.3, §8.15) | 4 (§2.1, §2.2, §4, §10) | ~7 | 30,000+ | ~5,000 |
| U18 | Append-only events, ULID and clock | ~8 | 3 (0002, 0004, 0007) | ~10 (§2, §2.2, §3.2–§3.4, §3.9, §4.1, §4.3, §8.6, §8.15) | 3 (§3 commit protocol, §7, §10) | ~5 | 30,000+ | ~5,000 |
| U19 | Commit validator | ~10 | 5–6 (0001, 0002, 0004, 0006, 0008, 0007) | 20+ (§2–§2.2, §3.2–§3.11, §4.1–§4.3, §5, §8.3–§8.8) | 4 | 15+ | 35,000+ | ~12,000 |
| U20 | Kernel acceptance (AT-033, AT-034, property test) | ~5 | 1 (0002) | ~4 (§2, §3.4, §3.11, §7) | 1 (§11) | ~4 | 30,000+ | ~3,500 |
| U21 | `doctor` v1 | ~6 | 3 (0002, 0004, 0007) | ~7 (§3.12, §3.13, §4.2, §5, §8.9, §8.10, §8.13) | 1 (§4 enforcement) | ~7 (kernel_state, commits, 4 doctor views, evidence) | 30,000+ | ~3,000 |
| U21, full §3.12 plus recipe and FX checks | as planned by Backend | ~20 | 5 (adds 0004, 0009 in full) | ~9 | 2 | ~12 | 40,000+ | n/a; split it (§6) |
| U22 | Observation store | ~9 | 2–3 (0002, 0007) | ~5 (§2.2, §3.5, §3.6, §4.1 import rows, §8.7) | 2 (§2.5, §3.4) | ~8 | 30,000+ | ~4,000 |
| U31 | Recipe acceptance and cross-entity check | ~15 | 5 (0001, 0004, 0006, 0007, 0008) | ~6 (§3.7–§3.9, §8.9, §8.14, §8.15) | 2 (§2.6, §11) | ~10 | 35,000+ | ~6,000 |
| U37 | Month-1 UI on the real app | ~12 | 3 (0003, 0006, 0007) | 3–5 today (§3, §4, for codes), 0 if U36 publishes an API page | 0 | 2 views | 20,000+ plus prototype PRs #20, #21, #23 (2,800 to 3,600 lines each) | ~3,000 plus one prototype |
| ADR 0009 FX | Single-currency accounts and FX Clearing | ~14 (8 FX terms plus currency, category, half terms) | 5 (0002, 0004, 0007, 0008, 0009) | ~7 (§1, §3.3, §3.4, §3.9, §3.12, §4.1, §4.3) | 2 (§2.3, §6) | ~6 | 35,000+ | ~7,000 |

Notes:

- **"Today"** assumes the usual brief, which points at whole files: GLOSSARY, KIS, delta, the schema and the cited ADRs. A careful contributor reads less. A new subagent with no prior context usually reads more, because nothing tells it what to skip.
- **"Scoped"** is the cited §s, the owned glossary slice, and the touched schema tables. Example for U21: KIS §3.12 (586 words), §3.13 (682), §4.2 (217), §5 (389), §8.9 (98) and §8.13 (196) come to about 2,200 words. Adding a glossary slice and the doctor views gives about 3,000 words. That is roughly 10× less than loading the whole set.
- **U19 stays large** even when scoped. It is the one place where every kernel invariant meets. That is a reason to keep the validator a single kernel-core module, with per-context rule functions registered into it (§3, FX). It is not a reason to split the kernel.
- **U31's definition is stale.** "Recipe and outbox acceptance pass, `doctor` pairing check" predates the ADR 0004 rewrite (#32), which withdrew the outbox and moved pairing to the application recipe check. The rewritten row should read "recipe acceptance, application recipe check".
- **All units were found** in the plan (U17 to U22, U31 and U37). ADR 0009 is on `main` (#33). There is no GitHub issue per unit. Issues #9 and #10 cover FX and multi-currency reporting at the backlog level.

## 3. Candidate bounded contexts

Each context gets a one-page `docs/contexts/<name>/INTERFACE.md` (§7). Below is what that page would hold. Operation names are the ones already in KIS §3.

### 3.0 Shared kernel (a small shared vocabulary, not a context)

- **Types:** `Money { amount_minor: positive int, currency: ISO 4217 }`; `Ulid`; `Revision` (`expected_revision`, `seq`); `CommitId`; dates (`effective_date` is the occurred date and assigns the period); the error envelope `{ code, class: user|import|bugcheck, refs, ids }` (KIS §4).
- **Terms (about 7):** Minor units, Commit, Effective date vs. recorded-at, Occurred date, Accounting entity, Entity id, Append-only journal.
- **Load:** KIS §2 (revision protocol) and the §4 preamble. Delta §3 (commit protocol).
- **Rule:** keep it under about 10 terms. Anything added here costs every context.

### 3.1 Ledger core (kernel)

- **Purpose:** one entity database's books: reference rows, events and postings, corrections, the commit seal, the numbered invariants that are not owned below, and the write freeze.
- **Interface:** `open(path, clock, ids)`, `begin_transaction(expected_revision)`, `append_event(event, postings)`, `correct(target, replacement)`, `commit()`, `record_write_freeze` / `clear_write_freeze`, and the kernel half of a recipe write (§3.9: an event carrying `recipe_instance_id` and counterpart fields, where an identical retry is a no-op). It also exposes a **validator rule hook**, so a context can add a per-event commit rule without editing core rules (FX uses this).
- **Errors:** `Unbalanced`, `InsufficientPostings`, `NonLeafCategory`, `CategoryParentInvalid`, `LedgerKindMismatch`, `CurrencyMismatch`, the reversal and supersede family, `StaleRevision`, `MissingExpectedRevision`, `WriteLocked`, `CommitSealed`, `IdempotencyConflict`, `InvalidShape` (closed token list), the §4.2 bugchecks.
- **Owns terms:** Posting, Ledger account, Account type, Category, Superseded, Party, Party id, Tracking mode, Black-box account, Contra-expense, Income, Valuation snapshot, Book basis.
- **Load:** KIS §1, §2, §3.1–§3.4, §3.9, §3.11, §3.13, §4, §5, §8.4–§8.8, §8.14, §8.15. Delta §3 and §4. ADRs 0002, 0003, 0005, 0008. Tables: `entity`, `commits`, `kernel_state`, `parties`, `party_aliases`, `reference_renames`, `recipes`, `financial_accounts`, `ledger_accounts`, `accounting_events`, `event_counterpart_expectations`, `journal_postings`, `tags`, `posting_tags`, `refund_links`, `valuation_snapshots`.
- **Must NOT need to know:** feed formats, match hints, suggestion ranking, the registry, other entity databases, rate sources, viewer-relative presentation, or screens.
- **Units:** U17 (done), U18, U19, U20, U35 (core part).

### 3.2 Observation import and evidence (kernel-hosted module)

- **Purpose:** store already-keyed feed facts and evidence bytes, idempotently. Void them.
- **Interface:** `import_observations(feed_id, rows keyed by external_id with row_hash)` returns inserted, identical no-op, or `ImportConflict`. Also `void_observation(observation_id, reason)`, the observation–posting link rule (INV-007) that `append_event` calls, and `put_evidence(bytes) -> hash`.
- **Errors:** `ImportConflict`, `CorroborationMismatch`, `VoidedObservation`, `ObservationAlreadyVoided`, `ObservationMismatch`, `ObservationDoubleConsumed`, `VoidWithoutReversal`, `OrphanEvidence`, `EvidenceHashMismatch`.
- **Owns terms:** Observation, Feed, Evidence, Void, Posted date, Origin.
- **Load:** KIS §2.2, §3.5, §3.6, §8.7. Delta §2.5 and §3.4. ADR 0007 (the adapter boundary). Tables: `feeds`, `import_runs`, `observations`, `observation_voids`, `posting_observations`, `evidence_objects`, `evidence_documents`, `evidence_deletions`, `evidence_links`.
- **Must NOT need to know:** recipes, settlement, Monarch CSV columns, or FX.
- **Anti-corruption layer:** each feed adapter (the Monarch adapter, U23) translates its file into observation rows. Monarch vocabulary (Reviewed, Pending, `Id`) stops at the adapter.
- **Units:** U22, U23 (adapter), U24.

### 3.3 Settlement and reconciliation (kernel-hosted module)

This context is not in the original candidate list. It is listed because U26, U32, U33 and U35 form a coherent cluster with their own terms.

- **Purpose:** settlement groups (draft, reconcile, dissolve), obligations and reimbursement claims, balance assertions and statements.
- **Interface:** `draft_group`, `reconcile_group`, `dissolve_group`, obligation open/settle/forgive, and `assert_balance(account, date, balance_minor)`.
- **Errors:** `DraftHasPostings`, `ReconciledGroupEmpty`, `NonzeroClearingResidual`, `PostingAlreadySettled`, `InvalidGroupTransition`, `SettlementPostingNotCurrent`, `DissolveWithoutReversal`, `DissolveWithoutRepost`, `LegsNotRegrouped`, `RecipeIncomplete`, `ClaimAccountMismatch`, `ClaimSettlementExceedsOpen`, `TransitiveInvalid`, `ClosedMonthViolation`, `StatementNotAgreed`.
- **Owns terms:** Reimbursement claim, Write-off, Expected movement, In-flight item.
- **Load:** KIS §3.7, §3.8, §3.10, §8.3, §8.8, §8.10–§8.12. Delta §2.6 and §6. ADRs 0001 and 0006. Tables: `settlement_*`, `obligation*`, `statements`, `balance_assertions`.
- **Must NOT need to know:** feed formats, the registry, or FX rates. FX Clearing rows are never settlement legs (ADR 0009).

### 3.4 Recipes and matching (layer 2, `accounting-recipes` crate)

- **Purpose:** turn observations into ChangeSets. Own match hints and suggestion ranking.
- **Interface:** `Recipe { code, version, build(observations, params) -> ChangeSet }` and `suggest(observation) -> ranked, deterministic list`. The ChangeSet is the only thing passed to the kernel.
- **Errors:** `UnknownRecipe`, and the kernel's codes passed through unchanged (`RecipeIncomplete`, `RefundShapeInvalid` and others).
- **Owns terms:** Recipe code, Match hint, Entity tag, Non-taxable fringe benefit, Investment income, Allocation (retired).
- **Load:** ADRs 0006, 0007 and 0008. KIS §3.3, for the ChangeSet shape only. The §4.1 code list. Delta §11 (acceptance tests).
- **Must NOT need to know:** triggers, §4.3 storage mapping, the freeze, other databases.
- **Units:** U25, U27, U28, U30.

### 3.5 Cross-entity recipes (layer 3, application)

- **Purpose:** write and correct the two halves of a cross-entity recipe instance. Run the recipe check across the registry.
- **Interface:** `write_instance(instance_id, half_a, half_b)` (ordered writes, retries are idempotent), `correct_instance(...)` (always both halves), and `recipe_check(registry) -> [status]`. Status is one of: ok, counterpart not visible, missing half, orphaned for 7 days, duplicate, mismatched counterpart, or mismatched amount (per currency).
- **Owns terms:** Recipe-instance id, Counterpart entity id, Counterpart expectation, Intercompany account and half amount, and the recipe-check half of "Health check".
- **Load:** ADR 0004. KIS §3.9 (the kernel's published language for one half), §8.9, §8.14, §8.15. The glossary "How these relate" paragraph. Tables: `accounting_events` (instance columns), `event_counterpart_expectations`, `entity`, `parties`.
- **Upstream:** ledger core (§3.9), recipes, and the registry in the application shell.
- **Must NOT need to know:** storage triggers, the commit seal internals, or FX rates. Under ADR 0009, cross-half FX is booked inside the converting side's own half, so this context sees only same-currency halves.
- **Units:** U29, U31.

### 3.6 FX (post-month-1; ADR 0009)

- **Purpose:** single-currency ledger accounts, the one FX Clearing account, derived rates, rate plausibility, and per-currency reporting of FX groups.
- **Interface, kernel side:** an account-currency rule and the FX Clearing shape rule, registered through the ledger core's validator rule hook, plus a `doctor_fx_clearing_shape` check. Errors: `CurrencyMismatch` (owned by core), and `InvalidShape` tokens `fx_clearing` and `account_currency`.
- **Interface, application side:** `derived_rate(event) -> exact rational, displayed to 7 places`; `RateSource` port `(pair, date) -> reference rate and tolerance | no reference | needs a person`; and `plausibility(groups) -> findings`, which are information or warning only and always exit 0.
- **Owns terms:** Account currency, Logical category vs. category ledger account, FX Clearing account, FX group, Derived rate, Effective balance, Net exchanged, Realized exchange gain/loss, Rate plausibility check, Unrealized gain/loss, FX translation.
- **Load:** ADR 0009, plus the ledger-core interface page. Delta §2.3 and §6.
- **Must NOT need to know:** the registry internals, rate providers (behind the port), settlement groups, or feed formats. Observation `original_amount_minor` is just a feed fact.
- **ACL:** `RateSource` is the anti-corruption layer toward any external rate data, including a person's gut check.

### 3.7 Doctor and checks

See §6. The kernel owns the runner and the finding shape. Contexts contribute checks.

### 3.8 Application shell: registry, CLI and JSON API (layer 3)

- **Purpose:** the per-login registry of entity databases, opening databases, sequencing doctor runs (on app open, after a cross-entity commit, before a backup), the CLI (U36), and the local JSON API that U37 consumes.
- **Interface:** `registry.resolve(entity_id | party_id) -> db handle | not visible`, CLI commands (`init`, `import`, `worklist`, `apply-recipe`, `group`, `assert`, `reconcile`, `correct`, `void`, `doctor`, `--json`), and the JSON API (one page, published by U36).
- **Owns terms:** Registry, and Visibility (viewer-relative presentation, ADR 0003).
- **Must NOT need to know:** triggers, invariants, or storage mapping. It calls context interfaces.

### 3.9 Read models and reporting (downstream, conformist)

- **Purpose:** the register and worklist (U34), the trial balance, per-currency month-end, and FX group listings.
- **Owns terms:** Worklist, Review state (derived per line), plus (from ADR 0009) the report-only meaning of Net exchanged.
- **Load:** schema views `register_events`, `observation_review_state`, `trial_balance`, `account_balances`, `settlement_group_current`, and later `fx_clearing_groups`.

### 3.10 UI (U37)

- **Purpose:** screens over the JSON API.
- **Load:** the JSON API page, the prototype Doug keeps, and the feedback log. It does **not** load the KIS, the schema or the ADRs. Error codes reach it as codes plus messages, which the shell maps.

## 4. Context map

```text
                     Shared kernel: Money, Ulid, Revision/Commit, dates, error envelope
                                         |
   [Feed adapters] --ACL--> [Observation import] --(same DB, same commit)--+
                                                                            v
   [Recipes & matching] --ChangeSet (customer/supplier)-->  [LEDGER CORE]  <--rule hook-- [FX kernel rules]
            ^                                                 ^      ^ (published language: KIS §3.9)
            |                                                 |      |
   [Cross-entity recipes] --uses--> [Application shell: registry, CLI, JSON API] --open host--> [UI]
            |                                                 |
            +-- recipe check ----+                            +-- runs --> [Doctor runner (kernel-owned)]
                                 v                                          ^ checks contributed by each context
                       [Read models & reporting] <--conformist-- views      |
   [FX app side] --ACL: RateSource port--> (rate data or a person)          +-- FX plausibility (app level)
   [Settlement & reconciliation]: kernel-hosted module, customer of ledger core, supplier to read models
```

| Relationship | Pattern | Why |
|---|---|---|
| Shared kernel to all Rust contexts | Shared kernel | Money, ids and the commit protocol are used everywhere. Keep it tiny. |
| Recipes to ledger core | Customer/supplier | Recipes emit ChangeSets. The kernel validates them and never interprets hints. |
| Feed adapters to observation import | Anti-corruption layer | Feed vocabulary stops at the adapter (ADR 0007). |
| Cross-entity to ledger core | Published language (KIS §3.9) | One half is an ordinary event plus instance and counterpart fields. |
| FX to ledger core | Partnership through a rule hook | ADR 0009 rule 3 is a kernel invariant, but its text and tests live with FX. |
| FX to rate data | Anti-corruption layer (port) | The provider is expected to change. It may be a person. |
| Shell to UI | Open host service (JSON API) | The UI never sees SQL or kernel internals. |
| Read models to schema views | Conformist | Reports follow the kernel's shapes. |

## 5. Split now or later

The recommendation: **split the docs now and the code later**, as the architect lean proposes, with two adjustments.

1. **The doctor seam is defined now, not after FX.** U21 is the next natural place to establish the runner and finding shape (§6). Waiting until after FX means U21 bakes the full §3.12 list into one function, and FX and recipes later edit it.
2. **Use module boundaries as U18 to U22 land, not crates.** New kernel code goes into `ledger`, `observations`, `settlement` and `doctor` modules from the start. Modules cost nothing and prevent a later refactor. Crate splits wait until the doctor and FX units have landed. One SQLite database per entity, with deferred cross-table foreign keys, means crates would mostly add friction now.

Evidence for doing the docs now:

- Every next kernel unit touches the 13,177-word KIS (§2.2).
- The scoped load is about 5 to 10 times smaller.
- The doc split is reversible, and it creates no merge conflict with code.

Evidence against splitting the KIS file itself now: U18 to U22 are about to cite it, and stable § anchors give most of the benefit for less churn. Revisit after U22.

## 6. Doctor: per-context checks under a kernel-owned runner

**Recommendation: yes, make doctor checks per-context modules.** U21 builds the runner plus the ledger-core checks only. It does not build the full §3.12 list in one unit.

### 6.1 Runner interface (kernel-owned)

```text
CheckId    "<context>.<name>", e.g. "ledger.unbalanced_events"
Severity   bugcheck | import | warning | info
Scope      single_db (kernel runner, read-only handle) | registry (application runner)
Check      { id, context, severity, scope, run(read_only_db) -> [Finding] }
Finding    { check_id, code (closed §4 code or context token), class, refs, ids }   // no amounts, names or memos
Report     { findings, exit_status }
```

- **Freeze policy stays in the kernel.** Only `bugcheck` findings from `single_db` checks trigger `record_write_freeze` (KIS §3.12, §3.13).
- **Exit status:** non-zero on `bugcheck` or `import` findings, which is today's §3.12 behaviour. `warning` and `info` never fail. That matches Doug's PR 33 answer for FX plausibility.
- **Two runners, one finding shape.** The kernel runner handles single-database read-only checks. The application runner handles checks that need the registry or a port. The application merges both into one report.
- **Context isolation.** A context registers checks with the runner. The runner does not import context logic.

### 6.2 KIS §3.12 checks, plus the checks already planned elsewhere, by context

| Check | Today in | Context | Scope | Severity | Lands in |
|---|---|---|---|---|---|
| `PRAGMA integrity_check` | KIS §3.12 | Ledger core (storage) | single_db | bugcheck | U21 |
| `PRAGMA foreign_key_check` | KIS §3.12 | Ledger core | single_db | bugcheck | U21 |
| `doctor_unsealed_rows` | KIS §3.12 | Ledger core (commit seal) | single_db | bugcheck | U21 |
| `doctor_unbalanced_events` (INV-001) | KIS §3.12 | Ledger core | single_db | bugcheck | U21 |
| `doctor_superseded_drift` | KIS §3.12 | Ledger core (corrections) | single_db | bugcheck | U21 |
| Active write freeze | KIS §3.12, §3.13 | Ledger core (runner) | single_db | bugcheck | U21 |
| `doctor_observation_double_consumed` (INV-007) | KIS §3.12 | Observation import | single_db | bugcheck | U22 |
| Evidence bytes against stored hashes | KIS §3.12 | Observation import and evidence | single_db | bugcheck | U22, or the first evidence unit |
| Staged evidence with no committed row | KIS §3.12 | Observation import and evidence | single_db | import | U22, or the first evidence unit |
| Regrouping drift (none yet, §8.10) | KIS §8.10 | Settlement | single_db | bugcheck | U26 or U35 |
| Recipe check: counterpart not visible, missing half, orphaned for 7 days, duplicate, mismatched counterpart, mismatched amount | ADR 0004, GLOSSARY "Health check" | Cross-entity | registry | info, or warning for orphaned | U29 and U31 |
| `doctor_fx_clearing_shape` | ADR 0009 | FX (kernel side) | single_db | bugcheck (`DoctorDrift`) | FX unit |
| Rate plausibility | ADR 0009 | FX (application side, `RateSource`) | registry/port | warning or info, always exits 0 | FX unit |
| Registry entry points to a missing file or a mismatched `entity` row | ADR 0004 | Application shell | registry | warning | U36 |

U21's acceptance ("planted drift detected") applies unchanged to the six ledger-core rows. The KIS §3.12 table can gain a "context" column when §3.12 is next edited. No rule changes.

## 7. Cheapest first step (concrete)

1. **One PR, docs only, about a day of Architect time.** Add `docs/contexts/README.md`, which is the context map from §4 plus a term-to-context index. Add three interface pages, `docs/contexts/ledger-core/INTERFACE.md`, `docs/contexts/observation-import/INTERFACE.md` and `docs/contexts/doctor/INTERFACE.md`. Each is 1,000 words or less, uses the §3 template, and links § anchors.
2. **Commit to stable anchors.** Add one rule to KIS §0: "§ numbers are stable. Never renumber. New sections take the next free number."
3. **Use the unit brief template** ([`docs/process/unit-brief-template.md`](../process/unit-brief-template.md)) for U18 onward. The brief lists the interface page plus the exact §s, and says what not to load.
4. **Add a context column to the plan's unit table.** Fix the U31 wording (§2.2 notes).
5. **Later (after U22):** move each context's glossary entries into its page, if Doug agrees (§8). Then decide whether to split the KIS file.

## 8. Decisions for Doug

1. **Doc split now, code split later.** Do you approve the per-context interface pages and context map now, with kernel modules (not crates) as U18 to U22 land? Recommended: yes.
2. **U21 scope.** Should U21 build the doctor runner plus the ledger-core checks only, with each other context adding its checks in its own unit? This changes Backend's plan to cover all of §3.12 in U21. Recommended: yes.
3. **Glossary home.** Should context-owned terms move into their context page, leaving a shared core of about 10 terms plus an index in `GLOSSARY.md`? The alternative is one glossary with a context tag on each entry. Recommended: move them after U22. Keep one definition per term either way.
4. **Fewer questions per PR.** May Architect pick defaults for reversible choices, and ask you only one-way-door questions, in one batch per PR? Recommended: yes.
