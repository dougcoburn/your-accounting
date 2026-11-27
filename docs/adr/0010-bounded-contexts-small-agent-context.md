# ADR 0010: Bounded contexts with one-page interfaces, so a change loads a small context

Status: **Proposed** (2026-11-26). Docs only. Nothing here is accepted until Doug merges it. No code moves in this change.

## Context

Doug's standing principle (2026-11-26): **smaller context = smarter agent work.** When the number of glossary terms and independent concepts needed to make a change gets large, use DDD to split the design into smaller bounded contexts with small interfaces, so an interface can be understood without its implementation details.

Measured on `main` at `24dee26` (details in [`docs/process/token-review-2026-11.md`](../process/token-review-2026-11.md)):

- The normative design set is about 48,000 words: `KERNEL_INTERFACE_SPEC.md` 13,177, `schema-v2.sql` 10,188, `SPEC.md` 4,901, ADR 0009 4,575, `SPEC-v2-delta.md` 4,276, `GLOSSARY.md` 2,947 (57 entries), `MAP.md` 2,664, ADR 0004 2,394, and the other seven ADRs 3,145 together.
- `GLOSSARY.md` grew from 773 to 2,947 words between 2026-11-20 and 2026-11-26, and 1,697 of those words came on 2026-11-26 (#32, #33). The kernel spec grew from 8,602 to 13,177 words between 2026-11-25 and 2026-11-26. ADR 0004 grew from 271 to 2,394 words on 2026-11-26.
- No document says which part of it a given unit needs. A unit brief therefore points at whole files. Every next kernel unit (U18 to U22) touches the 13,177-word spec.
- Captain and Architect account for about 63% of Grok Bot usage over the last 7 days (all work, not only this repo). The expensive rounds on this repo were coordination: relays, re-briefs, and patches regenerated against moving heads (see the token review).

## Decision (proposed)

1. **Name the bounded contexts now.** They are listed in [`docs/design/bounded-contexts.md`](../design/bounded-contexts.md), along with the context map. Each context owns its terms, its operations, its error codes and its doctor checks.
2. **Split the documentation now and the code later.** Each context gets `docs/contexts/<context>/INTERFACE.md`, one page of about 1,000 words or less. It has the purpose, the published operations and types, the error codes, the owned glossary terms, a **load list** (exact files and § anchors) and a **do-not-load list**. The existing specs stay the normative source until each section moves. An interface page cites § anchors. It does not paste their text.
3. **Do the cheapest first step first:** a context map plus three interface pages, for ledger core, observation import and doctor. Those are the contexts that U18 to U22 touch next. Other contexts get their page when their first unit is briefed.
4. **Keep one Rust crate for the kernel.** Inside it, put each context's code in its own module (`ledger`, `observations`, `settlement`, `doctor`, later `fx`), as U18 to U22 land. There is no crate split and no refactor of landed U17 code until the doctor and FX units have landed and the seams are proven.
5. **Doctor becomes a kernel-owned runner with per-context checks.** The kernel owns the runner, the finding shape and the freeze policy. Each context registers its own checks. U21 implements the runner plus the ledger-core checks only. Observation and evidence checks land with U22. The recipe check (ADR 0004) and FX checks (ADR 0009) land in their own units, under the same finding shape. The mapping is in the design doc, §6.
6. **Write rules that keep each context's documents small** (see [`docs/process/token-review-2026-11.md`](../process/token-review-2026-11.md) §4):
   - § anchors are stable.
   - Normative text is present tense, and history moves to changelogs.
   - Unit briefs use [`docs/process/unit-brief-template.md`](../process/unit-brief-template.md).
   - Doug's questions are batched once per PR.
   - Implementation patches are made against merged `main` only.

## Consequences

- An agent making a change in one context loads its interface page, the shared-kernel slice and the cited §s. By estimate, that is about 3,000 to 5,000 words instead of 30,000 or more for a kernel unit (design doc, §2).
- Cross-cutting decisions still need the full set. Examples are ADR 0004, ADR 0009, and any change to the commit protocol. Those are Architect and Principal work, and they should be rare.
- There is a duplication risk while text is copied into interface pages. Mitigation: the interface page holds only signatures, codes and § links, and every owned glossary term keeps a single definition. The glossary move (decision for Doug) makes the context page the single home.
- The month-1 plan's unit rows gain a "context" column. U31's "outbox" and "`doctor` pairing check" wording is already stale after the ADR 0004 rewrite. It should become "recipe-check acceptance (application context)" in a later plan edit. This PR does not edit the plan.

## Alternatives considered

- **Split code crates now.** Rejected for now. The SQLite schema is one database per entity, with deferred foreign keys across most tables. Crates would add build and visibility overhead before the seams are proven, and U18 to U22 would churn landed U17 code.
- **Do nothing and rely on better briefs.** Rejected. Without a published interface, every brief re-derives the scope from the 13,000-word spec, and every reviewer re-reads it.
- **Split the kernel spec file now into per-context files.** Deferred. It is a large text move against a spec that U18 to U22 are about to cite, and stable § anchors give most of the benefit for less churn. Revisit after U22.

## Open decisions for Doug

These are listed in the PR body and in [`docs/design/bounded-contexts.md`](../design/bounded-contexts.md) §8.
