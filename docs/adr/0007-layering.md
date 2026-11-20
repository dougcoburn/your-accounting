# ADR 0007: Layering and boundaries

Status: Proposed (2026-11-20)

## Decision
From the core outward:
1. **Kernel** (one per entity database): append-only journal, numbered invariants, commit validator. Knows nothing about tags, match hints, visibility or feeds.
2. **Recipes**: build ChangeSets from observations and own match hints.
3. **Application**: cross-entity orchestration (outbox protocol), `doctor` health check, wizards, access grants.
4. **Adapters**: Monarch CSV now; `bankstmt` JSON later. Observation natural key = feed id + external id; where Monarch has no stable id, the adapter derives one deterministically. A later feed row for the same account corroborates an existing observation instead of duplicating it.

## Consequences
Each layer is testable alone; the kernel can be property-tested with no feeds or UI.
