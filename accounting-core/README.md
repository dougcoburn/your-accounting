# Accounting Core — Rust DDD Workspace

This Cargo workspace is the direct Rust port of `accounting_kernel.py` and `accounting-core-kernel-v1.sql`.

## Workspace Layout

- `schema/accounting-core-kernel-v1.sql`: Frozen 6-table SQLite Core Kernel DDL with database triggers enforcing `SUM(debits) == SUM(credits)` per `(entity_id, currency)`, immutable posted entries, and zero-residual settlement groups.
- `crates/accounting-kernel`: Core DDD domain primitives (`Money`, `Currency`, `AccountElement`, `PostingSide`, `AccountRole`, `EntityVisibility`, `PostingPrimitive`, `JournalEntryDraft`, `EquationSnapshot`) and the SQLite transactional repository (`AccountingKernel`).
- `crates/accounting-recipes`: Application-ring `RecipeCompiler` and the full integration acceptance suite (`tests/acceptance_suite.rs` covering `AT-001`, `AT-003`, `AT-005..007`, `AT-013`, `AT-016`, `AT-019..020`, `AT-030`).
- `crates/accounting-cli`: M0 milestone deterministic CLI fixture runner.

## Quickstart

```bash
# Run all unit and integration acceptance tests across the workspace
cargo test --workspace

# Run the M0 CLI fixture runner
cargo run -p accounting-cli
```
