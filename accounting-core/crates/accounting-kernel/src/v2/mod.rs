//! v2 kernel surface for U17: typed ledger accounts, parties, and the write protocol.
//!
//! `stage_event` is the minimum stub INV-003 and recipe-half idempotency need.
//! U18 replaces it with `append_event`. An identical recipe retry returns the
//! existing event id and stages nothing. `correct_event` writes the reversal,
//! sets `superseded_by`, then writes the replacement. `doctor` is not
//! implemented (U21). `clear_write_freeze` runs the in-database bugcheck views
//! plus integrity and foreign-key checks, which is the part of that
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
