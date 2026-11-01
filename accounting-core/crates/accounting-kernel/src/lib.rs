//! `accounting-kernel` — Immutable DDD Core Kernel for Double-Entry Bookkeeping.

pub mod domain;
pub mod sqlite_repo;

pub use domain::{
    AccountElement, AccountRole, Currency, EntityVisibility, EquationSnapshot, JournalEntryDraft,
    KernelError, Money, PostingPrimitive, PostingSide,
};
pub use sqlite_repo::{AccountingKernel, KERNEL_SCHEMA_SQL};
