//! `accounting-kernel` — Pure DDD Core Kernel & Port Traits for Double-Entry Bookkeeping.

pub mod domain;
pub mod sqlite_repo;

pub use domain::{
    AccountElement, AccountRole, AccountingEquationSummary, CommittedJournalEntry, Currency,
    EntityVisibility, EntryFilter, EntryStatus, JournalEntryDraft, KernelError, LedgerAccount,
    LedgerStore, Money, PostingPrimitive, PostingSide,
};
pub use sqlite_repo::{SqliteLedgerAdapter, SqliteLedgerTransaction, KERNEL_SCHEMA_SQL};
