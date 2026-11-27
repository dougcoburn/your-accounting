# ADR 0011: The kernel stages only events and their lines

Status: **Accepted** (Doug, 2026-11-26, relayed by Captain). One open decision for Doug is listed below. Docs only: no schema or code change in this ADR.
Amends: [ADR 0007](0007-layering.md) (its 2026-11-25 amendment, which made import and stored tag rows kernel operations) and [ADR 0010](0010-bounded-contexts-small-agent-context.md) (context map: ledger core and observation import).

## Context

The kernel's `append_event` today (`docs/KERNEL_INTERFACE_SPEC.md` §3.3) stages an event with its postings, tags, observation links and refund links. `import_observations` (§3.6) and `void_observation` (§3.5) are also kernel operations. Evidence links are commit-scoped kernel rows. That puts four kinds of link in the kernel, along with their rules: INV-007 (an observation link must match its posting), INV-015 (refund cap) and the void rule. It also puts their codes and tokens there. The kernel's load list is bigger as a result, against the ADR 0010 principle.

"Line" in Doug's wording is a **posting** (`journal_postings` row). This ADR uses both words for the same thing.

## Decision (Doug, 2026-11-26)

1. **Not kernel concepts:** tags, file/evidence links, observation links and refund links.
2. **Context tables hold them.** They live in higher-layer (context or application) tables that reference a kernel event or line **by id**.
3. **`append_event` stages the event and its lines only.** An event's ADR 0004 recipe-half fields and its `event_counterpart_expectations` rows are part of the event (§3.9) and stay.

## Ownership (defaults per ADR 0010 decision 4, reversible)

| Concept | Tables (same entity DB file, context-owned) | Owner context | Unit |
|---|---|---|---|
| Observations and observation links | `feeds`, `import_runs`, `observations`, `observation_voids`, `posting_observations`; view `observation_review_state` | Observation import | U22 (#52) |
| Evidence and file links | `evidence_objects`, `evidence_documents`, `evidence_deletions`, `evidence_links` | Observation import and evidence | U22 (#52), or the first evidence unit |
| Refund links | `refund_links` (INV-015 cap, ADR 0008 refund shape) | Recipes | U27 (#57) |
| Tags | `tags`, `posting_tags`; `reference_renames` rows with `target_kind = 'tag'` | Application (tagging, in the application shell). Recipes and adapters supply tag hints. | U23 (#53) hints; U34/U36 storage |

**Why evidence goes to observation import, not the application.** Most evidence enters through import:
- `import_runs.document_id` already references `evidence_documents`;
- statements and CSVs are the source files of observations;
- the hash and orphan checks were already assigned to U22 (ADR 0010 §6.2).

User-attached receipts come from the application. They go through the same context's `put_evidence` and `link_evidence` operations. One owner keeps the byte store, hashes and links in one place, and the application doesn't need to know about storage.

**Kernel-owned (unchanged):**
- `entity`, `commits`, `kernel_state`, `parties`, `party_aliases`, `reference_renames`, `recipes`, `financial_accounts`, `ledger_accounts`;
- `accounting_events`, `event_counterpart_expectations`, `journal_postings`, `valuation_snapshots`;
- for now, the settlement, obligation and assertion tables.

Whether settlement and obligations follow the same pattern is a later, reversible scope question. It's not part of this decision.

## What moves out of the kernel

- **Operations.**
  - `import_observations` and `void_observation` become observation-import operations.
  - Observation, evidence, refund and tag links are staged by their owning context, not by `append_event`.
  - `void_observation` still reverses linked events through the kernel's `correct`.
- **Rules, kept in SQL as storage backstops and owned by the context:**
  - observation context: `posting_observations_valid` (INV-007), `posting_observations_not_voided`, `observations_currency_valid`, `observations_corroborate_valid`;
  - recipes: `refund_links_valid`, `refund_links_cap` (INV-015);
  - each context: the open-commit and append-only triggers on its own tables.
- **Two `commits_seal` clauses read `posting_observations`.** They are "a void must reverse every current event linked" and "a dissolved group's observed legs must be reposted". Both become observation-context validation, run before the seal. The SQL clauses stay as the backstop.
- **Codes.** They stay in the one closed list (KIS §4), which is the shared error envelope. Each code now names its owner:
  - observation import: `ImportConflict`, `CorroborationMismatch`, `ObservationMismatch`, `ObservationDoubleConsumed`, `VoidedObservation`, `ObservationAlreadyVoided`, `VoidWithoutReversal`, `OrphanEvidence`, `EvidenceHashMismatch`;
  - recipes: `RefundShapeInvalid`, `RefundCapExceeded`.
- **Tokens.** These `InvalidShape` tokens move to observation import: `void_reason`, `observation_direction`, `feed_kind`, `evidence_link_target`, `hash`. Some `unique_other` sources move with their tables: `tags.name` to the application, the `refund_links` pair to recipes, `evidence_deletions.sha256` to observation import.
- **Doctor.**
  - The context-table rows of `doctor_unsealed_rows` become each context's own unsealed check. The kernel runner keeps the kernel-table rows.
  - `doctor_observation_double_consumed` is already U22 (ADR 0010).
- **INV-013.** Transitive validation in `correct` asks each context to validate the links it owns (refunds, observation links), through the same hook as below.

## Defaults for the other questions (reversible, not asked)

- **Tamper evidence.** There is no hash chain in schema v2. The "seal" means three things:
  - the `commits` row is written last;
  - deferred `commit_id` foreign keys;
  - open-commit and append-only triggers.

  **Default:** context rows stay append-only, and each carries the `commit_id` of the commit it was written in, with its `recorded_at` taken from that commit. They are covered by the same seal as today, at no extra cost under option (i) below. A context may write its rows in a later commit than the event (for example, tagging later), but it never edits a row. Removals are new rows, such as a tombstone. A hash chain over all rows would be a separate decision. It isn't needed here.
- **Corrections.** When `correct` reverses an event and stages its replacement, each context re-links to the replacement lines **in the same operation**, through the hook. Links to the superseded event's lines are kept for history. Every "current" reading joins to `accounting_events` and filters `superseded_by IS NULL AND reverses_id IS NULL`, as `doctor_observation_double_consumed` already does, so old links do no harm.
- **Stable line ids (verified).** `journal_postings.id` is a ULID primary key with a format CHECK. The caller supplies it or the kernel allocates it from the injected id source, and it's preserved. Context foreign keys to lines are safe. One caveat: an identical §3.9 recipe retry returns the **stored** event and posting ids, so a context must link to the returned ids, not the ids it sent.

## Open decisions for Doug

1. **Atomicity of an event and its links (one-way door).** Today an event and its observation link commit together. With links moved out of the kernel, they could commit separately. If that happens, the observation looks unconsumed, and a **second interpretation** could double-book it, from a recipe suggestion or a person. A re-import of the same row cannot, because it is already a no-op on `(feed_id, external_id)`.
   - **(i) Same file, same transaction, through a narrow kernel hook (recommended).**
     - The kernel takes `BEGIN IMMEDIATE`, stages the event and lines, then calls each registered context's `stage(tx, new event and line ids, commit_id)`.
     - Before the seal, it calls `validate(tx, staged ids)`. Then it seals.
     - The kernel learns no link semantics. The interface grows by one trait with two methods, the same shape as the ADR 0010 validator rule hook that FX uses.
     - This keeps today's atomicity, the commit seal on link rows, the §2.2 no-op rule ("nothing staged by the kernel or any context") and every foreign key.
     - Cost: in-process context modules get a transaction handle limited to their own tables. External callers still never get a connection (KIS §1).
   - **(ii) Separate transaction after commit, made safe by idempotency.**
     - Writes are deterministic, keyed by the observation id: a repeated interpretation returns the existing event, and the link is then repaired.
     - A U22 doctor check finds imported-and-interpreted events with no link.
     - Cost: the window is real, and it needs a new idempotency column on the kernel's `accounting_events`, with retry and repair code in every context.
   - **(iii) A separate application DB file.** It loses foreign keys and atomicity. Rejected.

   **Recommendation: (i).** The kernel interface stays small: two callbacks, with no link vocabulary. Fall back to (ii) only if Doug wants contexts to never share the kernel's transaction.

This is the only question for Doug in this ADR. The others above are reversible defaults (ADR 0010 decision 4).

## Consequences

- KIS §1, §3.3–§3.6, §3.12, §4 and §8 are amended in the same PR (§8.16). `schema-v2.sql` is unchanged. The smoke test stays green.
- **Rust impact for Backend** (no change in this PR):
  - `v2/error.rs`: codes keep their names. An owner annotation, or a per-context grouping, comes later.
  - `v2/sqlmap.rs`: these move behind context modules:
    - the observation and refund RAISE mappings;
    - the `UNIQUE` mappings for `observations.feed_id, observations.external_id` (`ImportConflict`) and `observation_voids.observation_id` (`ObservationAlreadyVoided`);
    - the `void_reason`, `observation_direction`, `feed_kind` and `hash` tokens.
  - `v2/store.rs`:
    - `Subject::Observation` (the §2.2 no-op subject) becomes a context-supplied subject;
    - `blocking_bugcheck` drops `doctor_observation_double_consumed` from the kernel list, and the kernel-table part of `doctor_unsealed_rows` splits out;
    - `ReferenceKind::Tag` and `Feed` reads move to their contexts;
    - the hook from option (i) is added at `commit()` and in `correct_event`;
    - the `v2/tests.rs` test of `Subject::Observation` (line 1803 on `main`) moves with it.
  - `NewEvent` has no link fields today, so `append_event` (#71) needs no field removal.
- **Issues updated:** #48, #49, #51, #52, #53, #55, #57, #65.
