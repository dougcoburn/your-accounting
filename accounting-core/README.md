# Accounting Core — Rust DDD Workspace

This Cargo workspace provides a clean, decoupled Domain-Driven Design (DDD) architecture based on the [Kernel Interface Specification](docs/KERNEL_INTERFACE_SPEC.md).

## Features

- **Full Read Symmetry:** Query accounts, single entries, active entries by account with date filtering, and complete fixup lineages.
- **Fail-Fast Invariant Bugcheck:** Invariants are machine-guaranteed on mutation. `summarize_accounting_equation` provides structured reporting.
- **Painless Fixup Lineage:** Correct mistakes seamlessly via `replaces_entry_id`. Day-to-day balance queries see only the active reality, while forensic audits can inspect the complete edit trail.
- **Zero Storage Leak:** Kernel domain types contain no SQL or SQLite dependencies.

## Documentation

See [`docs/KERNEL_INTERFACE_SPEC.md`](docs/KERNEL_INTERFACE_SPEC.md) for full terminology definitions and architectural rules.

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

SQLite allocates short, per-prefix IDs (`je_1`, `pst_1`, `rx_1`) inside the
transaction. Draft entry and posting IDs are temporary references and are not
persisted. Use `post_entry_with_ids` to obtain the stored posting IDs before
linking a settlement group. `next_deterministic_id(prefix)` reserves other
application IDs in the same transaction; a rollback also rolls back that
reservation. A standalone reservation commits immediately and advances the
kernel revision.

Entries linked to a settled group cannot currently be replaced in isolation.
A coordinated correction operation for rebuilding group membership and audit
records remains to be added.
