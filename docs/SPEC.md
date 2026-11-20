# Accounting Core — Formal Specification v0.2

**Status:** Implementation baseline (review required for explicitly open decisions)  
**Date:** 2026-10-29  
**Architecture decision:** SQLite is the sole authoritative transactional store; one filesystem-backed evidence store; deterministic Rust core with CLI and service adapters.

## 0. Normative language and scope

**MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative. This specification describes an offline-first personal/multi-entity accounting system intended for automated implementation and executable acceptance testing.

**In scope for core:** dynamic accounting entities; financial accounts and movements; canonical transactions; drafts; versioned scenario definitions; balanced postings; classifications; transfers; obligations; flat settlement groups; statement and manual balance assertions; audited corrections; evidence metadata/storage; read-side reporting; authorization model and MCP service boundary.

**Not required for first core release:** polished web UI, OCR/LLM importer implementation, bank connectivity, tax computation, tax-law advice, cloud synchronization, remote MCP transport, DuckDB, or S3. Their contracts must remain implementable without changing the fundamental accounting model.

**Terminology:** “Division” is retained as a user-facing synonym for **accounting entity**. An entity is not necessarily a legally separate company. “Reconciled transaction” means its accounting interpretation has been committed; “reconciled statement” means its source activity and balance assertion are verified.

## 1. Architecture and authority

```text
CLI / UI / MCP clients / import agents
                |
       authenticated service API
                |
 authorization + approval workflow
                |
   deterministic domain/scenario engine
                |
    SQLite transaction boundary
                |
 SQLite (authoritative records)  ----  EvidenceStore interface
                                       |
                                 local filesystem (v0.2)
```

- Only the domain service can commit accounting changes; neither MCP clients nor importers receive arbitrary SQL access.
- SQLite transactions commit complete accounting operations, never partial postings.
- Original evidence bytes are outside SQLite. The evidence store has a single active backend. Offline migration to S3 MAY be implemented in a future version.
- Derived reports, cached balances, and indexes are reconstructible from committed state.
- Schema migrations are explicit, transactional where feasible, and tested against prior-version fixtures.
- Rust is the proposed implementation language, not a semantic requirement of the model.

## 2. Domain model

### 2.1 Accounting entities, parties, and reporting groups

`AccountingEntity` has `id`, `name`, `visibility` (`transparent` | `opaque`), lifecycle status, and timestamps. Entities are dynamic and unlimited; Personal, Household, and Business are examples, not reserved IDs.

- **Transparent:** owns a separately balanced ledger, category hierarchy, and budgets; may own financial accounts of any currency.
- **Opaque:** models a person or organization for obligations and reimbursements without maintaining its full books. An opaque entity MUST NOT require mirror-side imported bank movements.
- `Party` is a distinct vendor/person/organization identity with aliases and optional parent identity. A party MAY be associated with an opaque entity.
- `ReportingGroup` is a selectable set of transparent entities, independent of account ownership; entities MAY belong to multiple groups. Consolidation eliminates qualifying internal balances and flows; it MUST NOT erase third-party positions.

### 2.2 Financial accounts

`FinancialAccount`: `id`, `owner_entity_id`, `kind` (checking, savings, credit_card, brokerage, HSA, cash, gift_card, store_credit, other), `currency_code`, name, optional institution and external reference, status.

- Every financial account has **exactly one transparent owner at a time**.
- Ownership changes are dated, explicit accounting operations with audit history, not silent edits to historical ownership.
- Every financial account has exactly one denomination currency at a time. Currency is **not** an entity property.
- Gift cards, store credits, cash, and accounts without downloadable statements are valid.
- A financial account maps to one or more internal ledger accounts as required by the engine. The user does not maintain a traditional chart of accounts.

### 2.3 Currency and amounts

- Monetary amounts MUST use exact arithmetic: signed integers in currency minor units or exact decimal representation with declared scale. **Never IEEE floating point.**
- Accounting allocation magnitude MUST be nonnegative; direction is a separate debit/credit or economic-role field.
- Every posting has an explicit currency. Each committed event MUST balance **per transparent entity and currency**.
- Categories are independent of currency. Reports default to separate currency totals; no implicit cross-currency sum.
- FX transactions record actual source and destination amounts, currency pair, rate provenance, and any explicit conversion/valuation effects. The design MUST NOT assume a USD functional currency for each entity.
- Converted reports require target currency, valuation date, and rate policy. Native-currency reports are authoritative.

### 2.4 Categories, allocations, and tags

- Each transparent entity owns its own hierarchical category tree. Only leaf categories can be assigned to income/expense allocations.
- `Allocation`: stable ID, accounting interpretation/event, transparent entity, leaf category, nonnegative magnitude, currency, economic direction, optional notes. P&L allocations need exactly one entity and one category. Non-P&L journal postings do **not** require categories.
- A canonical transaction can have multiple allocations and offsetting economic components. Discounts ordinarily reduce the corresponding expense category via an explicit credit line, not a negative allocation magnitude.
- Tags are flat, free-form, zero or more at transaction or allocation level. Tags are **filter predicates**, not independent totals; nested AND/OR/NOT filters MUST avoid multiplying allocations through joins.
- Counterparties (e.g., Amazon) are parties, not tags.

### 2.5 Evidence, observations, and movements

`EvidenceDocument` is a logical uploaded document; `EvidenceObject` is its immutable content-addressed byte object. Distinct uploads MAY point to the same object.

`ExtractionRun` captures document ID/hash, importer identity, approximate implementation/configuration descriptor, timestamp, status, and resulting structured observations. Importer implementations MAY evolve in place. Exact re-execution is **not** guaranteed.

`Observation` is an immutable accepted source fact with original values, source document and extraction provenance, optional statement membership, and deduplication/match state. Re-extraction creates candidate observations for comparison; it MUST NOT silently replace accepted observations used in committed accounting.

`CanonicalTransaction` represents economic activity; it may be manually created without an imported observation. It has one or more `AccountMovement` records when financial accounts are involved.

`AccountMovement`: financial account, nonnegative magnitude, direction (increase/decrease to that account's signed balance), currency inherited from account, posting date, and optional supporting observation links. Multiple observations MAY support one movement (overlapping statements). One canonical transaction MAY contain movements from multiple accounts, entities, currencies, and posting dates.

`AccountingEvent` is a scenario-generated committed interpretation of one or more canonical transactions, or an accounting-only action without a canonical transaction (e.g., forgiving a receivable). It has scenario ID/version, effective-date override only when needed, postings, classifications, links, and revision history.

**Document-level evidence links** are many-to-many between documents and transactions/events; line-level PDF locators are not required. Invoices may inform allocations without line-level permanent pointers.

### 2.6 Internal ledger, obligations, and settlement groups

`LedgerAccount` is engine-managed: asset, liability, equity, income, expense, contra, clearing, etc. Each posting references a ledger account, transparent entity, currency, debit/credit direction, and nonnegative magnitude.

`Obligation` represents a real due-to/due-from or receivable/payable relationship, with creditor/debtor, currency, original amount, remaining amount, linked events, and settlement records. Cross-entity entries generate balanced postings for **each** transparent entity; opaque parties need no mirror books.

`SettlementGroup` is a **flat** reconciliation unit (no nested groups) referencing multiple transactions/events and required movement slots. It MAY span canonical transactions. Its draft records can include verified movements, missing-participant descriptors, and match suggestions. It MUST NOT introduce clearing balances into the committed ledger while incomplete.

A finalized group MUST have zero residual in every relevant `(transparent_entity, currency, clearing_account, settlement_group)` partition. Do not allow errors from unrelated groups to cancel.

**Membership policy:** all participants designated as required in a group reconcile **atomically as one unit**. A known participant MUST NOT be reconciled separately while its required group is incomplete. Previously reconciled transactions may be incorporated through a user-approved, dependency-aware audited correction. Genuine outstanding obligations are allowed when economically intended; synthetic AP/AR solely to bypass missing evidence is prohibited.

## 3. Lifecycle and state machines

### 3.1 Movements and transactions

- Movement verification: `unverified -> verified` when supported by acceptable statement/manual evidence; verification MAY precede transaction reconciliation.
- Canonical transaction: `draft -> validated -> reconciled`. `validated` is a transient or persisted review state; reconciliation atomically commits the interpretation.
- Accounting-only events need not have an observed financial movement.
- Imported observations, draft interpretations, and draft groups have **no official accounting effects**.

### 3.2 Settlement groups

`draft -> validated -> reconciled`. Draft may contain unresolved required participants; validation requires all required participants identified and all scenario/clearing invariants met. Reconciliation commits the complete group atomically. Corrections create audited revisions and may replace group membership, subject to revalidation.

### 3.3 Statements

`imported -> reviewed -> reconciled`. A statement may have a balance assertion while its transactions remain drafts. Final reconciliation requires:
1. All in-scope statement movements identified and appropriately verified/committed.
2. No unresolved duplicate, missing, or contradictory source activity.
3. Calculated ending balance equals asserted ending balance **exactly** in native currency.
4. All affected transaction/settlement dependencies complete.

Manual balance assertions have the **same mathematical authority** as statement-derived assertions; their provenance differs.

### 3.4 Dates

Posting date is the default accounting date. An accounting event MAY specify a separate effective date only when its scenario requires it. Statement balance calculations always use the account movement's posting date and the statement's defined cutoff convention, never an effective-date override.

## 4. Accounting invariants (machine-enforced)

**INV-001** Every committed journal event balances exactly by `(transparent_entity, currency)`.  
**INV-002** Every posting has one entity, ledger account, currency, nonnegative magnitude, and explicit debit/credit direction.  
**INV-003** All income/expense allocations use a leaf category owned by the same transparent entity.  
**INV-004** A financial account has one owner at any instant and one currency.  
**INV-005** Account movements match their account's currency; no silent FX.  
**INV-006** No draft produces committed postings or changes official balances.  
**INV-007** An accepted observation is not double-consumed as independent activity; multiple observations may corroborate one movement.  
**INV-008** A required settlement group is committed all-or-nothing, with zero residual per group/entity/currency/clearing account.  
**INV-009** A movement may be verified independently; transaction reconciliation requires its full scenario and any required group to be complete.  
**INV-010** Statement reconciliation requires exact balance agreement and no unresolved statement activity.  
**INV-011** Earlier balance assertions are never silently changed to repair later discrepancies. Initial inferred opening balance is explicit and revisable only through an approved correction.  
**INV-012** Correcting past classification is an audited correction; a new economic decision (e.g., gift forgiveness) is a new event.  
**INV-013** A committed correction validates all transitively affected obligations, groups, movements, statements, and journal postings before atomically replacing current state.  
**INV-014** Source evidence bytes and accepted source facts are never silently rewritten by accounting corrections.  
**INV-015** Refund-to-purchase links do not exceed refundable allocation magnitudes unless an explicit excess-credit scenario accounts for the difference.  
**INV-016** Report filters never duplicate an allocation, and never aggregate different currencies into a single native-currency total.  
**INV-017** Payroll clearing residual MUST be zero per committed payroll group/entity/currency, even if the aggregate clearing account balance happens to be zero.  
**INV-018** A client's MCP grant cannot be enlarged by that client or by a tool response; approval occurs in a trusted application interface.  
**INV-019** No unauthorized source record may influence an MCP client's returned aggregate or metadata.  
**INV-020** A committed evidence deletion retains audit metadata and cannot silently alter accounting entries.

## 5. Scenario contract and scenario registry

Scenarios are **code-defined**, shipped with application releases, stable IDs and semantic versions, typed inputs, deterministic postings, required movement roles, obligations, settlement requirements, and acceptance fixtures. Users and agents can propose new scenarios, but changing scenario code requires approval, tests, and a release. Historical events retain scenario ID/version and materialized postings; later code changes do not rewrite history.

Common contract:

```text
validate(input, snapshot) -> ValidatedPlan | DomainErrors
plan(validated_input, snapshot) -> ChangeSet
preview(ChangeSet) -> user-readable impacts and dependency list
commit(ChangeSet, expected_revision, approval) -> committed IDs | conflict
```

Required scenario families:
- ordinary purchase, income, category splits, discounts/contra-expense;
- transfer between controlled accounts (both sides required), same/cross-entity, same/cross-currency;
- credit card payment;
- expense funded by one transparent entity for another, generating paired interdivision balances;
- external opaque-party receivable/payable, partial and combined reimbursements;
- debt forgiveness/gift as **new** event;
- payroll gross-to-net including employee deductions, employer contributions, taxes, HSA/retirement movements, and zero-residual clearing;
- refund/return, partial and combined returns, refund to a different account/entity, goodwill excess credit;
- account ownership transfer and opening balance initialization/correction;
- owner draw, equity contribution, loan, and other non-P&L economic roles.

A scenario MAY combine multiple roles in one canonical transaction. Scenario selection is user-facing and never requires the user to specify journal debit/credit mechanics.

### 5.1 Refunds and purchase cohorts

Returns are new events linked to original purchase allocations; they do not retroactively mutate the purchase. The refund destination account and its owner may differ from the original payer. The original purchase allocation determines the expense reversal's entity/category; any cross-entity destination produces appropriate obligations.

Reports MUST support:
- **period accounting** (default): expense on purchase posting date, reversal on refund posting date;
- **purchase-cohort view**: eventual net cost of purchases originating in a selected period, including linked later returns;
- **gross activity** and optional exclusion of fully returned purchase/refund groups from listings, without deleting ledger entries.

## 6. Balance assertions and reconciliation arithmetic

`BalanceAssertion`: account, exact cutoff (date/time and inclusivity convention), amount, currency, source type (manual/statement), evidence reference, status, recorded time.

For adjacent assertions A and B on the same account and balance basis:

```text
expected_B = asserted_A + sum(signed verified/accepted account movements
                              with posting time in (A.cutoff, B.cutoff])
difference = asserted_B - expected_B
reconciled iff difference == 0 AND all required activity is resolved
```

Statement-source arithmetic may be checked against accepted observations while still drafts; **final** statement reconciliation uses committed/verified movement coverage. Balance basis (posted/ledger versus available) MUST be explicit; available balances are not automatically comparable with posted movements.

If the first known authoritative assertion lacks a prior opening, the engine MAY propose an inferred opening balance and explicit opening-equity accounting. It MUST NOT invent subsequent opening adjustments to conceal interval discrepancies.

## 7. Corrections and audit

Use a **mutable current-state ledger plus immutable revision/audit records**, not mandatory event sourcing.

An approved correction operation:
1. Loads current records at an expected revision.
2. Computes transitive dependencies (postings, allocations, obligations, refund links, settlement groups, movement matches, balance assertions, reconciled statements, reporting dates).
3. Produces a full proposed before/after change set and user-readable impact summary.
4. Revalidates all affected invariants, including any prior statement reconciliation.
5. Requires explicit trusted-interface approval, including when initiated by MCP.
6. Commits replacement current state **and** immutable before/after audit snapshots atomically in one SQLite transaction; otherwise commits nothing.

Audit record: actor/client, UTC timestamp, reason, affected IDs, before/after snapshots or equivalent complete reconstructible deltas, approval reference, operation ID. A correction MAY revise multiple previously reconciled transactions as one operation. New economic facts use new events rather than corrections.

No correction may leave an inconsistent dependent record. If dependencies cannot be resolved, return an actionable conflict; do not guess.

## 8. Evidence warehouse and importer behavior

- Exactly one active `EvidenceStore` per database, initially local filesystem. Storage API: `put`, `get`, `exists`, `delete`, `verify`. Stable evidence identity is independent of storage path.
- Store blobs by SHA-256 hash with deduplication. Keep separate upload/provenance records. Preserve originals unchanged unless explicitly deleted.
- Ingestion protocol: stage and hash file; atomically publish object in filesystem; commit SQLite metadata; recover/garbage-collect orphaned staged objects after interruption. Do not advertise an evidence record as available until its object is verified present.
- Deletion: require explicit audited approval when linked to reconciled records; show impacted records; retain metadata, hash, former links, deletion reason/time. Deleting bytes does not reverse accounting. Respect shared blob references.
- Backup/restore is a consistent pair of SQLite snapshot and evidence objects with a manifest and hash verification.
- Offline store migration: stop writers, copy all objects, verify hashes, prepare updated store config, atomically activate destination only after validation, retain source until confirmed; failures leave old store usable.
- Importers are mutable and need not be perfectly reproducible. Retain accepted structured observations used by reconciled records and lightweight provenance; discard unused intermediate artifacts according to retention policy.
- Agent-assisted importer changes require approval and regression comparison against prior evidence. Reimports propose differences, not silent changes.
- Evidence linking is document-level. Extracted invoice line items MAY inform allocations without persisted line-level source pointers.

## 9. Reporting and MCP

### 9.1 Reporting

- Entity P&L, balance sheet, budgets, category rollups, and selectable consolidated reporting.
- Default native-currency columns; conversion optional with explicit target, date, rate policy.
- Boolean filter AST: `AND`, `OR`, `NOT` over entity, account, party, category subtree, tag, date, and other approved dimensions.
- Tags are predicates only. Allocation identity is preserved under joins; never double count.
- Purchase-cohort and period accounting views are distinct query semantics, not ledger mutations.
- Consolidation eliminates eligible internal positions and transfers; external obligations remain.

### 9.2 MCP authorization boundary

MCP is an adapter over the same domain/reporting service as UI and CLI. No raw SQL or unrestricted filesystem access.

`ClientIdentity`, `Grant`, `PermissionRequest`, `Approval`, `AccessAudit` are first-class records. Deny by default. Grants specify operations, entity/account/category/date scopes, sensitivity tier (aggregates, transaction detail, evidence bytes), duration (`one_operation`, `session`, `persistent`), and expiration/revocation.

- Session grants default; persistent grants require explicit selection. One-time grants supported.
- Agents may **request** escalation; trusted UI approval is required. MCP requests or tool output cannot self-approve.
- Query authorization must apply **before aggregation** and to all source records that could influence a result. A restricted client cannot infer excluded records through totals, counts, error text, or search metadata.
- Reading transaction metadata does not imply reading PDFs or pay stubs.
- Mutation endpoints return draft proposals and impact previews. Reconciled changes require the same trusted approval/correction protocol.
- Log identity, operation, requested scope, decision, timestamp, and mutation outcome; minimize sensitive payload logging.
- Initial transport may be local. Future remote transport requires authenticated, encrypted connections, scoped tokens, and revocation.

Example service capabilities: `list_authorized_entities`, `query_spending`, `get_account_balances`, `get_retirement_contributions`, `get_transaction`, `request_evidence_access`, `propose_classification`, `propose_correction`, `request_permission_escalation`. Tool names are illustrative, not frozen API.

## 10. Proposed relational schema (logical SQLite baseline)

All IDs are opaque stable identifiers (UUID/ULID or equivalent). Timestamps use UTC plus explicit local posting-date semantics. `CHECK`, `UNIQUE`, `FOREIGN KEY`, and indexed lookup constraints enforce structural invariants; scenario/graph invariants are checked in application transactions.

| Table | Core columns / purpose |
|---|---|
| `entities` | id, name, visibility, status |
| `parties`, `party_aliases` | counterparty identity and aliases |
| `reporting_groups`, `reporting_group_members` | selectable consolidation membership |
| `financial_accounts`, `account_ownership_history` | account, kind, currency, dated owner |
| `ledger_accounts` | engine-managed account type, entity, optional financial account |
| `categories` | entity, parent, leaf enforcement, income/expense kind |
| `tags`, `transaction_tags`, `allocation_tags` | flat tagging |
| `evidence_objects`, `evidence_documents`, `evidence_uploads` | hash/blob, logical document, upload provenance |
| `extraction_runs`, `observations`, `observation_evidence` | accepted extracted facts and provenance |
| `statements`, `statement_observations`, `balance_assertions` | statement scope and balances |
| `canonical_transactions`, `account_movements`, `movement_observations` | deduplicated economic events and bank-side movements |
| `draft_interpretations` | proposed scenario inputs and review state |
| `accounting_events`, `event_transactions`, `journal_postings` | committed scenario and balanced ledger |
| `allocations`, `refund_allocation_links` | classified P&L and return relationships |
| `obligations`, `obligation_settlements` | due-to/due-from and settlements |
| `settlement_groups`, `settlement_participants`, `settlement_postings` | flat group membership and zero-residual scope |
| `evidence_links` | document-to-transaction/event links |
| `audit_operations`, `audit_record_changes`, `approvals` | immutable correction history and consent |
| `mcp_clients`, `mcp_grants`, `mcp_permission_requests`, `mcp_access_audit` | scoped agent access |
| `scenario_registry`, `schema_migrations`, `app_config` | scenario identifiers, migrations, store config |

**Important:** This is a **logical schema**, not yet executable DDL. Before implementation, define concrete columns, indexes, foreign-key actions, signed-balance convention, exact money representation, and the journal account mappings. Do not invent them silently in agent-generated code.

## 11. Executable acceptance specifications

Each test below is a contract. Implement as deterministic Rust integration tests against a fresh temporary SQLite database and temporary evidence directory. Verify both success results and absence of partial effects on failure.

**AT-001 — Split purchase across entities**  
Given a $150 Household-owned card charge, $100 Household groceries and $50 Business supplies; when reconciled; then both entities balance, expense categories match their entities, and Business owes Household $50.

**AT-002 — Category ownership rejection**  
Given an allocation assigned to a leaf category of another entity; when committed; then reject and persist no postings.

**AT-003 — Matched controlled transfer**  
Given checking withdrawal $500 and card payment credit $500 with independently verified observations; when grouped and reconciled; then one atomic accounting interpretation, no P&L.

**AT-004 — Missing transfer side**  
Given only the withdrawal; when reconcile is attempted; then remain draft and create no synthetic clearing/obligation entry.

**AT-005 — Opaque-party receivable**  
Given Household funds $100 for a child represented by an opaque entity; when reconciled; then Household has $100 receivable; no child-side account is required.

**AT-006 — Partial reimbursement**  
Given that receivable and a $40 deposit; when settled; then $60 receivable remains and no $40 income is recorded.

**AT-007 — Forgiveness is new event**  
Given $60 outstanding; when user approves forgive-as-gift; then a new $60 gift expense event closes the receivable without rewriting original expense.

**AT-008 — Tag filter does not multiply**  
Given one Amazon transaction with multiple allocations and tags; when filtered by party AND tag; then each eligible allocation appears once.

**AT-009 — Statement discrepancy blocks**  
Given March ending $5,000, April posted net -$800, April asserted $4,150; then difference -$50 blocks reconciliation; March assertion unchanged.

**AT-010 — Initial inferred opening**  
Given first statement net -$1,200 and ending $5,000; when approved; then explicit opening $6,200 and balanced opening-equity postings.

**AT-011 — Duplicate evidence bytes**  
Given same PDF uploaded twice; then one blob and distinct upload provenance; no duplicate committed movement.

**AT-012 — Scenario upgrade stability**  
Given an event committed with scenario v1; when v2 ships; then its postings and scenario reference remain unchanged.

**AT-013 — Unbalanced posting rejection**  
Given one entity/currency unbalanced; then commit rolls back every related record.

**AT-014 — Unresolved statement activity**  
Given a balance-matching statement with one unresolved movement; then final statement reconciliation fails.

**AT-015 — Native currencies and categories**  
Given USD and EUR purchases in the same Travel category; then report shows separate USD/EUR totals and never silently adds them.

**AT-016 — Cross-entity refund destination**  
Given Household's $100 expense and a $100 refund to Personal-owned gift card; then Household expense reverses, Personal gift-card asset increases, and matching interdivision obligation is recorded.

**AT-017 — Period versus purchase cohort**  
Given $100 September purchase and $100 October linked return; period report shows September expense and October reversal; September cohort view shows $0 eventual net cost.

**AT-018 — Refund cap**  
Given a $100 purchase allocation and linked refunds totaling $110 with no excess-credit scenario; then reject excess $10 and preserve prior state.

**AT-019 — Payroll clearing**  
Given gross pay, employer HSA contribution, deductions, taxes, and matched net/HSA movements; then payroll group balances and its clearing residual is zero per group/entity/currency.

**AT-020 — Missing payroll movement**  
Given a required HSA movement absent; then payroll settlement group stays draft and no clearing postings commit.

**AT-021 — Group-wide atomicity**  
Given two known purchases, a $300 reimbursement, and one unresolved required purchase; then all remain in draft group; no partial reconciliation.

**AT-022 — Previously reconciled transaction joins group**  
Given a committed purchase later recognized as part of a reimbursement group; when approved correction runs; then its prior interpretation is replaced atomically, audit preserved, group revalidated.

**AT-023 — Transitive correction failure**  
Given a corrected purchase allocation that would invalidate an already-settled obligation; then reject unless the complete dependent change set is valid and approved.

**AT-024 — Manual balance assertion parity**  
Given a manual gift-card balance assertion; then apply the same exact balance arithmetic as a statement assertion and retain manual provenance.

**AT-025 — Evidence deletion**  
Given a receipt linked to reconciled accounting; when deletion approved; then bytes may be removed, audit/hash/link metadata remains, and accounting postings do not change.

**AT-026 — Re-extraction stability**  
Given a committed transaction supported by extracted invoice data; when improved extraction yields different items; then original accounting remains unchanged until audited correction.

**AT-027 — MCP aggregate isolation**  
Given an agent authorized only for Household; when it requests consolidated spending; then Business records cannot affect returned totals/counts/errors.

**AT-028 — MCP permission escalation**  
Given a client with session read-only access; when it requests persistent evidence access; then access remains denied until trusted UI approval, and revocation takes effect.

**AT-029 — FX transfer**  
Given USD withdrawal and EUR deposit; when scenario commits; then actual amounts and rate evidence are retained, and all entity/currency journal partitions balance without implicit cross-currency addition.

**AT-030 — Zero residual cannot cross-cancel**  
Given two settlement groups with residuals +$10 and -$10 in the same clearing account; then neither group can finalize despite aggregate clearing balance zero.

**AT-031 — Failure injection and crash recovery**  
Given simulated failure after evidence blob publication but before SQLite metadata commit; when recovered; then no dangling committed evidence record and orphan is safely recoverable/collectable.

**AT-032 — Audit and revision conflict**  
Given concurrent proposed corrections to the same reconciled event; when first commits; then second fails expected-revision check, with no partial changes.

## 12. Implementation milestones and definition of done

**M0 — Foundation:** Rust workspace; SQLite migrations; entity/account/category schema; exact money types; journal engine; invariant tests; CLI fixture runner.

**M1 — Transactions:** canonical transactions, movements, observations, drafts, scenario registry; ordinary expense/income/split/discount; entity obligations; reporting basics.

**M2 — Reconciliation:** matched transfers; flat settlement groups; balance assertions; statement reconciliation; payroll/returns; FX scenario contracts.

**M3 — Corrections and evidence:** transitive dependency analysis; atomic audited replacement; filesystem evidence store; ingestion/recovery; re-extraction comparison.

**M4 — Service and MCP:** authorized query services; granular scoped grants; permission escalation via trusted approval; audit and redaction tests.

**M5 — UI/importer agents:** review workflows, PDF extraction, rules suggestions, import regression testing, richer reports.

**Done means:** all applicable acceptance tests pass on an empty and upgraded database; foreign keys enabled; money uses exact arithmetic; failed commits leave no partial state; deterministic fixtures run offline; threat-model tests cover MCP data leakage; backups restore consistent database+evidence.

## 13. Explicit unresolved design decisions

These are **not** authorization for an implementation agent to invent behavior:

1. **Money representation:** integer minor units versus arbitrary-precision fixed decimal, including nonstandard scales and FX rounding.
2. **Journal account model for multi-currency FX:** precise clearing/valuation posting template that balances each entity/currency without creating fictitious gains.
3. **Statement cutoff semantics:** timezone, posting-day ordering, and which balances (posted/ledger/available) can be asserted.
4. **Refund linkage accounting:** allocation-level refund cap across multiple refunds, shipping/tax/discount allocation policy.
5. **Payroll tax classification:** withholding as tax expense versus prepaid tax/receivable by user preference and reporting jurisdiction; employer benefits may be income or informational depending on reporting context.
6. **Entity reporting across currencies:** how native-currency multi-column balance sheets express retained earnings and equity without a designated functional currency.
7. **Account ownership transfer:** treatment of historic statements and transfers at the ownership-change boundary.
8. **Settlement participation constraints:** whether a movement/event can belong to multiple groups and how double participation is prevented.
9. **Correction effects on reconciled statements:** when reclassification leaves them valid versus when movement/date changes require explicit re-reconciliation.
10. **Approval UX and MCP authentication:** local trusted approval mechanism and remote transport choices.
11. **Budget semantics:** whether budgets are entity/category/currency-specific and how carryovers work.
12. **Data retention:** deletion policy for unused extraction artifacts and user-configurable audit/evidence retention.

**Implementation policy:** Resolve an open decision in a small ADR and add failing tests before writing behavior dependent on it. No “reasonable default” may silently redefine an accounting invariant.

## 14. Architectural decisions recorded

- **ADR-001:** SQLite is the sole authoritative transactional database. DuckDB is optional future analytics, not part of core.
- **ADR-002:** Exactly one active filesystem evidence store; future S3 via explicit offline migration.
- **ADR-003:** Dynamic transparent and opaque entities; each financial account has one transparent owner and one currency.
- **ADR-004:** Separate native currencies in reports; categories are currency-independent.
- **ADR-005:** Mutable current-state ledger with immutable correction audit, not mandatory event sourcing.
- **ADR-006:** Flat, zero-residual settlement groups are atomic reconciliation boundaries.
- **ADR-007:** Original evidence and accepted observations remain distinct from canonical transactions and accounting events.
- **ADR-008:** MCP is a scoped, auditable service adapter with trusted interactive approval.
- **ADR-009:** Importer logic need not be perfectly reproducible; accepted accounting inputs and provenance must be retained.
