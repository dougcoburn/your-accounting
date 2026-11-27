# Accounting Core — Rust DDD Workspace

This Cargo workspace provides a clean, decoupled Domain-Driven Design (DDD) architecture based on the [v2 Kernel Interface Specification](../docs/KERNEL_INTERFACE_SPEC.md). The original v1 `LedgerStore` contract is kept for history as [`docs/KERNEL_INTERFACE_SPEC-v1-SUPERSEDED.md`](docs/KERNEL_INTERFACE_SPEC-v1-SUPERSEDED.md).

## Features

- **Full Read Symmetry:** Query accounts, single entries, active entries by account with date filtering, and complete fixup lineages.
- **Fail-Fast Invariant Bugcheck:** Invariants are machine-guaranteed on mutation. `summarize_accounting_equation` provides structured reporting.
- **Painless Fixup Lineage:** Correct mistakes seamlessly via `replaces_entry_id`. Day-to-day balance queries see only the active reality, while forensic audits can inspect the complete edit trail.
- **Zero Storage Leak:** Kernel domain types contain no SQL or SQLite dependencies.

## Documentation

See the v2 kernel interface spec, [`docs/KERNEL_INTERFACE_SPEC.md`](../docs/KERNEL_INTERFACE_SPEC.md) at the repository root, for the current contract. The v1 file [`docs/KERNEL_INTERFACE_SPEC-v1-SUPERSEDED.md`](docs/KERNEL_INTERFACE_SPEC-v1-SUPERSEDED.md) describes the v1 `LedgerStore` API below and is historical.

## Design docs

Design checkpoint (Formal Spec v0.2 and the month-1 wayfinder). These live at the repository root; the reference schema is [`docs/schema-v0.2.sql`](../docs/schema-v0.2.sql) and is not wired into the code.

- [`docs/SPEC.md`](../docs/SPEC.md) — Formal Spec v0.2, the design baseline
- [`docs/design/MAP.md`](../docs/design/MAP.md) — wayfinder map
- [`GLOSSARY.md`](../GLOSSARY.md) — shared vocabulary
- [`docs/adr/`](../docs/adr/) — architecture decision records 0001–0008

## Quickstart

```bash
cargo test --workspace
cargo run -p accounting-cli
```

## Application transaction boundary

Application services can read `store.revision()`, prepare a preview, then call
`store.begin_transaction(expected_revision)`. The returned handle implements
`LedgerStore`, so entries and clearing groups can be staged alongside application
records through `with_connection`. `commit()` makes them durable and advances the
revision; dropping the handle rolls everything back. A failed ledger mutation
prevents that handle from committing.

An entry posted inside the handle is `active` for validation within that
transaction but is invisible to other database connections until commit. The
application must validate all required participants before committing a group.
The current settlement primitive checks linked clearing postings; it does not
itself know which observed movements a workflow requires.

`next_deterministic_id(prefix)` reserves short, per-prefix IDs (`je_1`,
`pst_1`, `rx_1`) from SQLite for kernel or application records. An application
can reserve IDs before constructing a draft, set them with `with_id`, and use
those same IDs in its domain records and settlement groups. Constructor-generated
draft references are replaced on posting for older callers. Reservations made
inside a transaction roll back with it; standalone reservations commit at once
and may leave gaps if a later workflow fails. Reserving an ID alone does not
advance the ledger revision.

Entries linked to a settled group cannot currently be replaced in isolation.
A coordinated correction operation for rebuilding group membership and audit
records remains to be added.
