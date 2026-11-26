//! v2 kernel surface for U17: typed ledger accounts, parties, and the write protocol.
//!
//! `stage_event` and `apply_inbox` are the minimum stubs INV-003 and the inbox
//! idempotency rule need. U18 replaces `stage_event` with `append_event`.
//! The inbox stub compares the §3.9 payload hash and does not build a counterpart leg.
//! `doctor` is not implemented (U21). `clear_write_freeze` runs the in-database
//! bugcheck views plus integrity and foreign-key checks, which is the part of
//! that precondition this unit can see.

mod error;
mod sqlmap;
mod store;
mod types;

pub use error::{Class, Code, Error};
pub use store::{
    inbox_payload_hash, CommitInfo, CommitResult, EntityRow, FinancialAccountRow, FreezeOutcome,
    InboxApply, InboxRow, Kernel, LedgerAccountRow, NewEntity, NewEvent, NewFinancialAccount,
    NewLedgerAccount, NewParty, NewPosting, PartyRow, Subject, Transaction,
};
pub use types::{
    AccountType, Clock, FinancialKind, FixedClock, IdSource, InboxAction, LedgerKind, PartyLabel,
    ReferenceKind, SequenceIds, Side, TrackingMode,
};

#[cfg(test)]
mod tests;
