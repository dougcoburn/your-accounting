//! v2 kernel surface: typed ledger accounts, parties, append-only events, and doctor.
//!
//! `append_event` stages one current event and its postings. Context rows that
//! reference them are staged by registered participants in the same transaction
//! (kernel spec §3.14). `recorded_at` and kernel-allocated ids come from the
//! clock and id source injected at `open`. An identical recipe retry returns
//! the existing event id and stages nothing. `correct_event` writes the
//! reversal, sets `superseded_by` once, then writes the replacement, then asks
//! each participant to re-link. `doctor` runs the ledger-core checks and any
//! checks a context registered. It does not repair rows and it does not clear
//! a freeze.

mod contexts;
mod doctor;
mod error;
mod participant;
mod sqlmap;
mod store;
mod types;

pub use contexts::{
    observation_links, EvidenceRecord, EvidenceTarget, ImportBatch, ImportOutcome, NewEvidence,
    NewFeed, ObservationImport, ObservationInput, ObservationLink, CHECK_DOUBLE_CONSUMED,
    CHECK_EVIDENCE_HASH, CHECK_ORPHAN_EVIDENCE, CHECK_UNSEALED as CHECK_IMPORT_UNSEALED,
};
pub use doctor::{
    exit_status, DoctorCheck, DoctorRunner, Finding, Scope, Severity, CHECK_FOREIGN_KEYS,
    CHECK_FREEZE, CHECK_INTEGRITY, CHECK_SUPERSEDED_DRIFT, CHECK_UNBALANCED, CHECK_UNSEALED,
};
pub use error::{Class, Code, Error};
pub use participant::{
    CommitStagedSet, ContextParticipant, ContextTx, KernelStaged, ParticipantPayload,
    ParticipantPayloads, StageOp, StageOutcome, StagedContextRow, StagedKernelEvent,
};
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
