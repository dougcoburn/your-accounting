//! v2 kernel surface: typed ledger accounts, parties, and append-only events.
//!
//! `append_event` stages one current event and its postings. `recorded_at` and
//! kernel-allocated ids come from the clock and id source injected at `open`.
//! An identical recipe retry returns the existing event id and stages nothing.
//! `correct_event` writes the reversal, sets `superseded_by` once, then writes
//! the replacement. That update is the only one the schema accepts. `doctor`
//! is not implemented (U21). `clear_write_freeze` runs the in-database bugcheck
//! views plus integrity and foreign-key checks, which is the part of that
//! precondition this unit can see.

mod error;
mod sqlmap;
mod store;
mod types;

pub use error::{Class, Code, Error};
pub use store::{
    CommitInfo, CommitResult, CounterpartExpectation, EntityRow, FinancialAccountRow,
    FreezeOutcome, Kernel, LedgerAccountRow, NewEntity, NewEvent, NewFinancialAccount,
    NewLedgerAccount, NewParty, NewPosting, PartyRow, Subject, Transaction,
};
pub use types::{
    AccountType, Clock, FinancialKind, FixedClock, IdSource, LedgerKind, PartyLabel, ReferenceKind,
    SequenceIds, Side, TrackingMode,
};

#[cfg(test)]
mod tests;
