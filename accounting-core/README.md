# Accounting Core — Rust DDD Workspace

This Cargo workspace provides a clean, decoupled Domain-Driven Design (DDD) architecture based on the [Kernel Interface Specification](docs/KERNEL_INTERFACE_SPEC.md).

## Architecture & Ports

1. **Pure Double-Entry Core Kernel (`crates/accounting-kernel`)**:
   - Defines pure Value Objects, Aggregates, and the `pub trait LedgerStore` Port.
   - Zero storage leaks: `KernelError` contains no database-specific types.
   - Includes `SqliteLedgerAdapter` implementing `LedgerStore` backed by `schema/accounting-core-kernel-v1.sql`.

2. **Application Recipes Ring (`crates/accounting-recipes`)**:
   - Consumes the kernel strictly through `LedgerStore`.
   - Records workflow execution provenance in `recipe_executions` (`crates/accounting-recipes/schema/accounting-recipes-v1.sql`).
   - Contains integration acceptance tests (`tests/acceptance_suite.rs`).

3. **CLI Runner (`crates/accounting-cli`)**:
   - Milestone M0 CLI fixture runner demonstrating in-memory double-entry posting and balance proof reporting.

## Documentation

See [`docs/KERNEL_INTERFACE_SPEC.md`](docs/KERNEL_INTERFACE_SPEC.md) for full terminology definitions, mathematical invariant equations, and architectural boundary rules.

## Quickstart

```bash
cargo test --workspace
cargo run -p accounting-cli
```
