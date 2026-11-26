# ADR 0007: Layering and boundaries

Status: Proposed (2026-11-20; amended 2026-11-25 to match docs/KERNEL_INTERFACE_SPEC.md)

## Decision
From the core outward. Layers 1 and 3 are amended below (2026-11-25); where the list and the amendment differ, the amendment wins.
1. **Kernel** (one per entity database): append-only journal, numbered invariants, commit validator. Knows nothing about tags, match hints, visibility or feeds.
2. **Recipes**: build ChangeSets from observations and own match hints.
3. **Application**: cross-entity orchestration (outbox protocol), `doctor` health check, wizards, access grants.
4. **Adapters**: Monarch CSV now; `bankstmt` JSON later. Observation natural key = feed id + external id; where Monarch has no stable id, the adapter derives one deterministically. A later feed row for the same account corroborates an existing observation instead of duplicating it.

## Consequences
Each layer is testable alone; the kernel can be property-tested with no feeds or UI.

## Amendment (2026-11-25): kernel operations, per the kernel interface spec
The merged kernel interface spec (`docs/KERNEL_INTERFACE_SPEC.md` §1, §3.6, §3.9, §3.12, §8.2) and schema v2 put more inside the per-entity kernel than layer 1 above. That spec is authoritative now, so this ADR follows it.

- **Kernel operations (layer 1, widened).** The kernel's operations include:
  - the append-only journal, commit seal and numbered invariants
  - `import_observations`, which stores already-keyed observations: natural key `feed_id` + `external_id`, `row_hash`, idempotent re-import, `ImportConflict` on a changed hash
  - writing the cross-entity outbox row in the same commit as the first leg, and the idempotent inbox apply in the counterpart database
  - `doctor`
  - storing feed rows, tag rows and access grants as facts, without interpreting them
- **Still outside the kernel:**
  - **Recipes (layer 2):** match hints, suggestion ranking and ChangeSet building
  - **Application (layer 3):**
    - deciding to start the counterpart leg and retrying it
    - any UI around a cross-entity gap
    - viewer-relative presentation (ADR 0003)
    - wizards
    - which access grants to issue
  - **Adapters (layer 4):** feed file formats, and deriving an external id where a feed has none
- **Invariants that do not change:**
  - **Every kernel write goes through the commit protocol.** That means `begin_transaction(expected_revision)` with the caller's revision, staging, and the commit row written last and sealed, so the change is all or nothing (ADR 0002; spec §2). Import, outbox and inbox are writes like any other. An inbox apply runs its own `begin_transaction` on the counterpart database with that database's caller-supplied revision. There is no side door and no second connection that skips the foreign-key assert.
  - **Import is not matching.** `import_observations` stores facts and nothing else. It never creates postings, groups or settlements, so a new observation is `unreviewed` unless the same caller-built changeset also interprets it. Matching stays a recipe-owned suggestion that a person confirms (ADR 0006).
  - **`doctor` is read-only.** It opens no write transaction and repairs nothing. It never compacts the journal or rewrites `superseded_by`. When a finding is bugcheck class, the kernel refuses later writes on that database until a clean `doctor`, but doctor itself changes no row (spec §3.12, §5). For cross-database pairing it reads counterpart databases named explicitly by the caller. Each database stays isolated for writes (ADR 0004).
- **Why amend rather than supersede:** see the PR that introduced this amendment. In short, layers 2 to 4 and the observation natural key are unchanged. Only the kernel/application line in layers 1 and 3 moves, and this repo records that kind of change as a dated amendment (compare ADR 0002).

