//! v2 kernel surface: typed ledger accounts, parties, append-only events, and doctor.
//!
//! `append_event` stages one current event and its postings. `recorded_at` and
//! kernel-allocated ids come from the clock and id source injected at `open`.
//! An identical recipe retry returns the existing event id and stages nothing.
//! `correct_event` writes the reversal, sets `superseded_by` once, then writes
//! the replacement. That update is the only one the schema accepts. `doctor`
//! runs the ledger-core checks and any checks a context registered. It does
//! not repair rows and it does not clear a freeze.

mod doctor;
mod error;
mod sqlmap;
mod store;
mod types;

pub use doctor::{
    exit_status, DoctorCheck, DoctorRunner, Finding, Scope, Severity, CHECK_FOREIGN_KEYS,
    CHECK_FREEZE, CHECK_INTEGRITY, CHECK_SUPERSEDED_DRIFT, CHECK_UNBALANCED, CHECK_UNSEALED,
};
pub use error::{Class, Code, Error};
pub use store::{
    CommitInfo, CommitResult, CounterpartExpectation, DoctorReport, EntityRow, FinancialAccountRow,
    FreezeOutcome, Kernel, LedgerAccountRow, NewEntity, NewEvent, NewFinancialAccount,
    NewLedgerAccount, NewParty, NewPosting, PartyRow, Subject, Transaction,
};
pub use types::{
    AccountType, Clock, FinancialKind, FixedClock, IdSource, LedgerKind, PartyLabel, ReferenceKind,
    SequenceIds, Side, TrackingMode,
};

#[cfg(test)]
mod tests;
