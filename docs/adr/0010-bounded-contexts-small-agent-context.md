# ADR 0010: Bounded contexts with one-page interfaces, so a change loads a small context

Status: **Accepted** (Doug, 2026-11-26; decisions on PR #73 relayed by Captain). Docs only. No code moves in this change.

## Context

Doug's standing principle (2026-11-26): **smaller context = smarter agent work.** When the number of glossary terms and independent concepts needed to make a change gets large, use DDD to split the design into smaller bounded contexts with small interfaces, so an interface can be understood without its implementation details.

Measured on `main` at `24dee26` (details in [`docs/process/token-review-2026-11.md`](../process/token-review-2026-11.md)):

- The normative design set is about 48,000 words: `KERNEL_INTERFACE_SPEC.md` 13,177, `schema-v2.sql` 10,188, `SPEC.md` 4,901, ADR 0009 4,575, `SPEC-v2-delta.md` 4,276, `GLOSSARY.md` 2,947 (57 entries), `MAP.md` 2,664, ADR 0004 2,394, and the other seven ADRs 3,145 together.
- `GLOSSARY.md` grew from 773 to 2,947 words between 2026-11-20 and 2026-11-26, and 1,697 of those words came on 2026-11-26 (#32, #33). The kernel spec grew from 8,602 to 13,177 words between 2026-11-25 and 2026-11-26. ADR 0004 grew from 271 to 2,394 words on 2026-11-26.
- No document says which part of it a given unit needs. A unit brief therefore points at whole files. Every next kernel unit (U18 to U22) touches the 13,177-word spec.
- Captain and Architect account for about 63% of Grok Bot usage over the last 7 days (all work, not only this repo). The expensive rounds on this repo were coordination: relays, re-briefs, and patches regenerated against moving heads (see the token review).

## Decision (Doug, 2026-11-26)

1. **Name the bounded contexts now.** They are listed in [`docs/design/bounded-contexts.md`](../design/bounded-contexts.md), along with the context map. Each context owns its terms, its operations, its error codes and its doctor checks.
2. **Split the documentation now and the code later.** Each context gets `docs/contexts/<context>/INTERFACE.md`, one page of about 1,000 words or less. It has the purpose, the published operations and types, the error codes, the owned glossary terms, a **load list** (exact files and § anchors) and a **do-not-load list**. The existing specs stay the normative source until each section moves. An interface page cites § anchors. It does not paste their text.
3. **Do the cheapest first step first:** a context map plus three interface pages, for ledger core, observation import and doctor. Those are the contexts that U18 to U22 touch next. Other contexts get their page when their first unit is briefed.
4. **Keep one Rust crate for the kernel.** Inside it, put each context's code in its own module (`ledger`, `observations`, `settlement`, `doctor`, later `fx`), as U18 to U22 land. There is no crate split and no refactor of landed U17 code until the doctor and FX units have landed and the seams are proven.
5. **Doctor becomes a kernel-owned runner with per-context checks.** The kernel owns the runner, the finding shape and the freeze policy. Each context registers its own checks. U21 implements the runner plus the ledger-core checks only. Observation and evidence checks land with U22. The recipe check (ADR 0004) and FX checks (ADR 0009) land in their own units, under the same finding shape. The mapping is in the design doc, §6.
6. **Write rules that keep each context's documents small** (see [`docs/process/token-review-2026-11.md`](../process/token-review-2026-11.md) §5):
   - § anchors are stable.
   - Normative text is present tense, and history moves to changelogs.
   - Unit briefs use [`docs/process/unit-brief-template.md`](../process/unit-brief-template.md).
   - Architect defaults reversible (two-way-door) choices and states them. Only one-way-door questions go to Doug, batched once per PR (decided item 4 below).
   - Implementation patches are made against merged `main` only.

## Consequences

- An agent making a change in one context loads its interface page, the shared-kernel slice and the cited §s. By estimate, that is about 3,000 to 5,000 words instead of 30,000 or more for a kernel unit (design doc, §2).
- Cross-cutting decisions still need the full set. Examples are ADR 0004, ADR 0009, and any change to the commit protocol. Those are Architect and Principal work, and they should be rare.
- There is a duplication risk while text is copied into interface pages. Mitigation: the interface page holds only signatures, codes and § links, and every owned glossary term keeps a single definition. After U22, the glossary move (decided item 3 below) makes the context page the single home.
- The month-1 plan's unit rows gain a "context" column. U31's "outbox" and "`doctor` pairing check" wording is already stale after the ADR 0004 rewrite. It should become "recipe-check acceptance (application context)". A separate cleanup PR fixes the plan; this PR does not edit it.

## Alternatives considered

- **Split code crates now.** Rejected for now. The SQLite schema is one database per entity, with deferred foreign keys across most tables. Crates would add build and visibility overhead before the seams are proven, and U18 to U22 would churn landed U17 code.
- **Do nothing and rely on better briefs.** Rejected. Without a published interface, every brief re-derives the scope from the 13,000-word spec, and every reviewer re-reads it.
- **Split the kernel spec file now into per-context files.** Deferred. It is a large text move against a spec that U18 to U22 are about to cite, and stable § anchors give most of the benefit for less churn. Revisit after U22.

## Decided (Doug, 2026-11-26)

1. **Split the docs now; split the code later, as kernel modules, not crates.** The per-context interface pages and the context map start now. Kernel code goes into modules as U18 to U22 land. A crate split is not planned.
2. **U21 is the doctor runner plus the ledger-core checks only.** That means the check runner and registry interface, plus the six ledger-core checks. Every other context adds its checks in its own unit: observation and evidence in U22, the recipe check in U29 and U31, and FX shape and plausibility in the FX unit. Warning and info findings never fail doctor.
3. **Glossary home.** After U22, context-owned terms move into their context pages. `GLOSSARY.md` keeps a small shared core (about 10 terms) plus an index from each term to its context. Each term keeps one definition.
4. **Delegation.** Architect may choose and state defaults for reversible choices. Only one-way-door questions (schema shape, invariants, anything that changes stored data) go to Doug, batched once per PR.

The same list is in [`docs/design/bounded-contexts.md`](../design/bounded-contexts.md) §8.
