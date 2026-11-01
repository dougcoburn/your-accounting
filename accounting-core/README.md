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
