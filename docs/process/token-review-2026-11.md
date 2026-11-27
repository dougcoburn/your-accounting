# Token and effort review, 2026-11 (first pass)

**Status:** Proposed with [ADR 0010](../adr/0010-bounded-contexts-small-agent-context.md), 2026-11-26. Captain runs a weekly Friday retro on this. Each retro updates §5 and §6.

**Principle (Doug, 2026-11-26):** smaller context = smarter agent work.

## 1. Data and caveats

- **Usage share, last 7 days, by bot.** This is Grok Bot usage across **all** work, not only this repo: Captain 33%, Architect 30%, Principal 5%, Backend 5%, Full-stack 4%, Frontend 3%, DevOps 3%, PM 3%, QA 2%.
- **There is no per-task breakdown.** Everything in §3 is an **estimate**. It is inferred from PR and commit history, doc growth, working-patch sizes and the number of decision rounds. It is ranked relative to other tasks. No token counts are claimed.
- **Sources:**
  - `gh pr list` and `gh pr view` for #19 to #34.
  - `git log` word counts for `GLOSSARY.md`, `docs/KERNEL_INTERFACE_SPEC.md`, `docs/SPEC-v2-delta.md` and ADR 0004.
  - Sizes of the working patch and PR-body drafts kept on the build machine.
  - The decision relays visible to Architect.
- **No GitHub reviews or comments exist** on any of these PRs (0 reviews and 0 comments on #19 to #34). All review and coordination ran through agent messages, which is why it shows up as Captain and Architect usage.

## 2. What the shares say

- **Coordination and design dominate.** Captain and Architect are about 63% combined. The implementers (Backend, Full-stack and Frontend) are about 12%.
  - Part of this is the phase. Month 1 has been design-heavy: U15, U16, the plan, and ADRs 0004 and 0009. U18 onward has not started.
  - Part of it is overhead. Every Doug decision travelled Doug → Captain → Architect, and often on to Backend and back. Each hop re-loaded the large documents.
- **Principal at 5% is low for the review load.** That is consistent with review happening as one or two message rounds per PR rather than line-level GitHub review. It is cheap, but it pushes consistency work back onto Architect. For example, #27 was a whole follow-up PR to fix six inconsistencies flagged on #26.
- **The lever is the number of times a large document is re-read, not the writing itself.** The design set is about 48,000 words (design doc §2.1). An agent turn that re-reads the KIS, the glossary and an ADR to answer one question can cost more than the edit that follows it.

## 3. Per-task estimates (relative)

| Task | Evidence | Relative cost (est.) | Why it was expensive (or not) |
|---|---|---|---|
| **#31 U17 and #32 ADR 0004 rewrite (recipe-halves churn)** | #31 has 4 commits (+5,366/−174, 11 files). 3 of those commits follow design changes made in #32 while #31 was open: drop row-level `entity_id`, drop the outbox, per-currency expectations. #32 has 3 commits, 7 files, and 3 PR-body drafts (about 35 KB, 33 KB and 8 KB). 3 patch generations were made for #31 against its moving head: a row-`entity_id` schema patch of about 30 KB, a recipe-halves patch set of about 23 + 21 + 15 KB, and a multi-currency patch of about 23 KB. ADR 0004 went from 271 to 2,394 words. The glossary went from 1,250 to 2,211 words. Doug made at least 3 decision rounds. | **Highest** | The implementation PR was open while its ADR was rewritten. Each design turn meant a relay, a re-brief of Backend, a regenerated patch against a new head, and a re-read of ADR 0004, the glossary and KIS §3.9 and §8.15 by both agents. The cross-cutting terms (party id, entity id, counterpart entity id, two meanings of recipe id) had to be defined to settle naming between two agents. |
| **#33 ADR 0009 FX Clearing** | 4 commits, 6 files (ADR, GLOSSARY, KIS, delta, MAP, ADR 0004). 3 Q&A rounds with Doug via Captain: 8 questions, then 8 answers that included a new design request (cross-half FX), then 1 decision. ADR 0009 is 4,575 words, the largest ADR. The glossary grew by 736 words. | **High** | The questions were asked in one round and the design request arrived in the answers, so a third round followed. The ADR carries worked examples, a rejections table, the schema delta and alternatives in one file, so every round re-read all of it. It also edited 5 other docs, because FX touches currency, categories, recipes and reporting. |
| **#25, #26 and #27 (kernel interface spec, ADR 0007 amendment, six inconsistencies)** | 3 PRs in about a day. The KIS went from 8,602 to 11,913 words. ADR 0007 gained stacked amendments. | **Medium–high** | The same scope rule is stated in several places: ADR 0007, KIS §0, §1 and §8.2, and the delta. Changing one meant reconciling all of them, and the reviewer found six mismatches. |
| **#19 U15 schema v2 and delta** | 2 commits, +1,473 lines, 7 files. One Principal round with 2 required fixes and answers to Q1–Q5. Q6 was decided later. | **Medium** | Mostly inherent: a 1,030-line schema written from scratch. There was one review round. |
| **#29 month-1 plan** | 4 commits. Refreshed twice against a moving `main`. 4,285 words. | **Medium** | "As of" restatements of other PRs' outcomes go stale on every merge. Units still cite whole files. |
| **#34 `recipe_party` token** | 1 commit, +42/−19 | **Low** | Small, scoped, against merged `main`. This is the target shape. |
| **Captain's relays** (all PRs) | Every Doug decision above went through Captain. | **High, spread out** | Each relay restates context. A relayed answer that opens a new question costs a full round. |
| **Re-briefing new subagents** | Work rounds are run by delegated subagents that start without context, as this review was | **Medium, repeated (est.)** | Without a load list, each new subagent rediscovers scope by reading whole files. There are two files named `KERNEL_INTERFACE_SPEC.md` (v1 legacy and v2), which is a mis-load risk. |

## 4. Where the tokens went (themes)

1. **Whole-file loads.** No unit says which §s it needs. Design doc §2.2 estimates 30,000+ words per kernel unit today, against about 3,000 to 5,000 when scoped.
2. **Cross-cutting terms.** Each of the 57 glossary entries is global. #32 and #33 together added 1,697 glossary words. Most of those words serve one context: cross-entity or FX.
3. **History inside normative text.** KIS §0 and §1 spend about 800 words re-explaining which amendment widened what. The KIS §9 changelog is about 800 words more. ADR 0007 has three stacked amendments. A reader must load the history to learn the current rule.
4. **Implementation against an unsettled design.** #31 versus #32.
5. **Q&A spread over rounds.** #33 took 3 rounds and #32 at least 3.
6. **Relay hops.** Doug → Captain → Architect → Backend and back, with no persistent shared place such as PR comments.

## 5. Process changes that cut tokens

Ordered by expected saving per unit of effort. The savings are estimates.

### 5.1 Doc layout

- **One page per context.** `docs/contexts/<context>/INTERFACE.md` is 1,000 words or less. It holds operations, types, error codes, owned terms, a load list of § anchors, and a do-not-load list (design doc §3 and §7).
- **Split the glossary by context.** Keep a shared core of about 10 terms. Each other term lives in exactly one context page, and `GLOSSARY.md` becomes an index. This is a decision for Doug.
- **Stable § anchors.** Never renumber a section. Briefs and reviews then cite §s instead of pasting text.
- **Present-tense normative text.** History moves to a changelog or an ADR "History" section that readers skip. ADR 0007 is then rewritten once as current text, with its amendments in History.
- **ADR size budget.** Context, decision and consequences together stay at 1,500 words or less. Worked examples and rejection tables go in an appendix file that implementers load only when needed. Applying this to ADR 0009 would cut its default load by about half (estimate).
- **One name per file.** Rename or clearly label `accounting-core/docs/KERNEL_INTERFACE_SPEC.md` (the v1 legacy spec) so nobody loads it instead of the v2 spec. This is a follow-up, not done here.

### 5.2 Brief template

Use [`unit-brief-template.md`](unit-brief-template.md) for every unit brief and every subagent brief. It lists:

- the base SHA,
- the context,
- the interface page,
- the exact §s to load,
- what not to load,
- the decisions already made (as links),
- the acceptance tests,
- and the stop conditions.

A new subagent reads the brief, not a transcript.

### 5.3 Fewer review rounds

- **One batch of questions per PR.** Ask all of Doug's questions in the PR body, at once. Before asking, list the follow-ups each answer could raise, so a design request like cross-half FX is anticipated in round one.
- **Defaults for reversible choices.** Architect decides two-way-door questions, states the default, and asks Doug only about one-way doors. These include schema shape, invariants, and anything that changes stored data. This needs Doug's agreement (decision 4).
- **Principal reviews the interface and the diff.** The review covers the context page and the changed §s, not the whole KIS. A cross-cutting ADR gets one full review.
- **Short PR bodies.** About 400 words: what changed, the load list, decisions needed. The long drafts (#32's 35 KB and 33 KB bodies) were mostly restated spec.

### 5.4 Fewer relay hops

- **Relay verbatim.** Captain passes Doug's words plus the PR link and § anchors, not a restatement.
- **Coordinate in PR comments.** Architect and Backend coordinate schema naming in review comments on the PR, which is one persistent place, instead of separate agent messages that each need re-briefing. Captain still sets who works on what.
- **No bilateral loops.** Agents use the PR thread rather than agent-to-agent message rounds.

### 5.5 Patches against merged `main` only

- **Design first, then implementation.** A design PR (ADR or spec) merges before the implementation PR for the same rule is opened or updated. If they must overlap, the implementation PR waits and rebases once.
- **No regenerated patches.** Patches for another agent's PR are not regenerated against a moving head. The agent who owns the PR applies the merged design.

## 6. Metrics for the Friday retro

Each of these is cheap to collect from `gh` and `git`. Captain's weekly retro tracks them per PR:

| Metric | Source | Target |
|---|---|---|
| Doug Q&A rounds per PR | PR body history | 1 |
| Commits per docs PR | `gh pr view --json commits` | 2 or fewer |
| Patches regenerated for another agent's PR | Working notes | 0 |
| Words in the load list of each unit brief | The brief | 5,000 or fewer (U19 is the exception) |
| Growth of `GLOSSARY.md` and the KIS per week | `git log` plus `wc -w` | Flat; new terms land in context pages |
| Usage share: coordination (Captain + Architect) vs. implementation | Grok Bot usage report | Trending down as U18+ starts |
