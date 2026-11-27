//! Closed error taxonomy from `docs/KERNEL_INTERFACE_SPEC.md` §4.
//!
//! Display is `code`, `class`, `refs`, `ids`, and the `InvalidShape` token.
//! SQL text, amounts, names, and payloads are not stored on the value.

use std::fmt;

/// `user` | `import` | `bugcheck`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    User,
    Import,
    Bugcheck,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Import => "import",
            Self::Bugcheck => "bugcheck",
        }
    }
}

/// One name from kernel spec §4.1 or §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    MissingExpectedRevision,
    StaleRevision,
    RevisionMismatch,
    WriteLocked,
    DatabaseUninitialized,
    Unbalanced,
    InsufficientPostings,
    ClosedMonthViolation,
    NonLeafCategory,
    CategoryParentInvalid,
    LedgerKindMismatch,
    CurrencyMismatch,
    RecordedAtMismatch,
    ReversalTargetInvalid,
    ReversalDoesNotCancel,
    SupersedeInvalid,
    SupersedeMissing,
    EventInsertedSuperseded,
    VoidWithoutReversal,
    VoidedObservation,
    ObservationAlreadyVoided,
    ObservationMismatch,
    ObservationDoubleConsumed,
    ImportConflict,
    CorroborationMismatch,
    DraftHasPostings,
    ReconciledGroupEmpty,
    NonzeroClearingResidual,
    PostingAlreadySettled,
    InvalidGroupTransition,
    SettlementPostingNotCurrent,
    DissolveWithoutReversal,
    DissolveWithoutRepost,
    LegsNotRegrouped,
    RecipeIncomplete,
    ClaimAccountMismatch,
    ClaimSettlementExceedsOpen,
    RefundShapeInvalid,
    RefundCapExceeded,
    AppendOnlyViolation,
    CommitSealed,
    RenameTargetMissing,
    UnknownTarget,
    UnknownRecipe,
    SecondEntity,
    ValuationNotAllowed,
    IdempotencyConflict,
    TransitiveInvalid,
    InvalidId,
    DuplicateId,
    InvalidMoney,
    InvalidCurrency,
    InvalidDate,
    InvalidShape,
    StatementNotAgreed,
    CrossEntityGap,
    OrphanEvidence,
    ForeignKeysDisabled,
    UnsealedRows,
    PartialCommit,
    ValidatorMissed,
    DoctorDrift,
    IntegrityFailed,
    ForeignKeyCheckFailed,
    EvidenceHashMismatch,
    UnmappedStorageError,
    StorageFault,
    WriteFrozen,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingExpectedRevision => "MissingExpectedRevision",
            Self::StaleRevision => "StaleRevision",
            Self::RevisionMismatch => "RevisionMismatch",
            Self::WriteLocked => "WriteLocked",
            Self::DatabaseUninitialized => "DatabaseUninitialized",
            Self::Unbalanced => "Unbalanced",
            Self::InsufficientPostings => "InsufficientPostings",
            Self::ClosedMonthViolation => "ClosedMonthViolation",
            Self::NonLeafCategory => "NonLeafCategory",
            Self::CategoryParentInvalid => "CategoryParentInvalid",
            Self::LedgerKindMismatch => "LedgerKindMismatch",
            Self::CurrencyMismatch => "CurrencyMismatch",
            Self::RecordedAtMismatch => "RecordedAtMismatch",
            Self::ReversalTargetInvalid => "ReversalTargetInvalid",
            Self::ReversalDoesNotCancel => "ReversalDoesNotCancel",
            Self::SupersedeInvalid => "SupersedeInvalid",
            Self::SupersedeMissing => "SupersedeMissing",
            Self::EventInsertedSuperseded => "EventInsertedSuperseded",
            Self::VoidWithoutReversal => "VoidWithoutReversal",
            Self::VoidedObservation => "VoidedObservation",
            Self::ObservationAlreadyVoided => "ObservationAlreadyVoided",
            Self::ObservationMismatch => "ObservationMismatch",
            Self::ObservationDoubleConsumed => "ObservationDoubleConsumed",
            Self::ImportConflict => "ImportConflict",
            Self::CorroborationMismatch => "CorroborationMismatch",
            Self::DraftHasPostings => "DraftHasPostings",
            Self::ReconciledGroupEmpty => "ReconciledGroupEmpty",
            Self::NonzeroClearingResidual => "NonzeroClearingResidual",
            Self::PostingAlreadySettled => "PostingAlreadySettled",
            Self::InvalidGroupTransition => "InvalidGroupTransition",
            Self::SettlementPostingNotCurrent => "SettlementPostingNotCurrent",
            Self::DissolveWithoutReversal => "DissolveWithoutReversal",
            Self::DissolveWithoutRepost => "DissolveWithoutRepost",
            Self::LegsNotRegrouped => "LegsNotRegrouped",
            Self::RecipeIncomplete => "RecipeIncomplete",
            Self::ClaimAccountMismatch => "ClaimAccountMismatch",
            Self::ClaimSettlementExceedsOpen => "ClaimSettlementExceedsOpen",
            Self::RefundShapeInvalid => "RefundShapeInvalid",
            Self::RefundCapExceeded => "RefundCapExceeded",
            Self::AppendOnlyViolation => "AppendOnlyViolation",
            Self::CommitSealed => "CommitSealed",
            Self::RenameTargetMissing => "RenameTargetMissing",
            Self::UnknownTarget => "UnknownTarget",
            Self::UnknownRecipe => "UnknownRecipe",
            Self::SecondEntity => "SecondEntity",
            Self::ValuationNotAllowed => "ValuationNotAllowed",
            Self::IdempotencyConflict => "IdempotencyConflict",
            Self::TransitiveInvalid => "TransitiveInvalid",
            Self::InvalidId => "InvalidId",
            Self::DuplicateId => "DuplicateId",
            Self::InvalidMoney => "InvalidMoney",
            Self::InvalidCurrency => "InvalidCurrency",
            Self::InvalidDate => "InvalidDate",
            Self::InvalidShape => "InvalidShape",
            Self::StatementNotAgreed => "StatementNotAgreed",
            Self::CrossEntityGap => "CrossEntityGap",
            Self::OrphanEvidence => "OrphanEvidence",
            Self::ForeignKeysDisabled => "ForeignKeysDisabled",
            Self::UnsealedRows => "UnsealedRows",
            Self::PartialCommit => "PartialCommit",
            Self::ValidatorMissed => "ValidatorMissed",
            Self::DoctorDrift => "DoctorDrift",
            Self::IntegrityFailed => "IntegrityFailed",
            Self::ForeignKeyCheckFailed => "ForeignKeyCheckFailed",
            Self::EvidenceHashMismatch => "EvidenceHashMismatch",
            Self::UnmappedStorageError => "UnmappedStorageError",
            Self::StorageFault => "StorageFault",
            Self::WriteFrozen => "WriteFrozen",
        }
    }

    pub fn class(self) -> Class {
        match self {
            Self::ImportConflict
            | Self::CorroborationMismatch
            | Self::CrossEntityGap
            | Self::OrphanEvidence => Class::Import,
            Self::ForeignKeysDisabled
            | Self::UnsealedRows
            | Self::PartialCommit
            | Self::ValidatorMissed
            | Self::DoctorDrift
            | Self::IntegrityFailed
            | Self::ForeignKeyCheckFailed
            | Self::EvidenceHashMismatch
            | Self::UnmappedStorageError
            | Self::StorageFault
            | Self::WriteFrozen => Class::Bugcheck,
            _ => Class::User,
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "MissingExpectedRevision" => Self::MissingExpectedRevision,
            "StaleRevision" => Self::StaleRevision,
            "RevisionMismatch" => Self::RevisionMismatch,
            "WriteLocked" => Self::WriteLocked,
            "DatabaseUninitialized" => Self::DatabaseUninitialized,
            "Unbalanced" => Self::Unbalanced,
            "InsufficientPostings" => Self::InsufficientPostings,
            "ClosedMonthViolation" => Self::ClosedMonthViolation,
            "NonLeafCategory" => Self::NonLeafCategory,
            "CategoryParentInvalid" => Self::CategoryParentInvalid,
            "LedgerKindMismatch" => Self::LedgerKindMismatch,
            "CurrencyMismatch" => Self::CurrencyMismatch,
            "RecordedAtMismatch" => Self::RecordedAtMismatch,
            "ReversalTargetInvalid" => Self::ReversalTargetInvalid,
            "ReversalDoesNotCancel" => Self::ReversalDoesNotCancel,
            "SupersedeInvalid" => Self::SupersedeInvalid,
            "SupersedeMissing" => Self::SupersedeMissing,
            "EventInsertedSuperseded" => Self::EventInsertedSuperseded,
            "VoidWithoutReversal" => Self::VoidWithoutReversal,
            "VoidedObservation" => Self::VoidedObservation,
            "ObservationAlreadyVoided" => Self::ObservationAlreadyVoided,
            "ObservationMismatch" => Self::ObservationMismatch,
            "ObservationDoubleConsumed" => Self::ObservationDoubleConsumed,
            "ImportConflict" => Self::ImportConflict,
            "CorroborationMismatch" => Self::CorroborationMismatch,
            "DraftHasPostings" => Self::DraftHasPostings,
            "ReconciledGroupEmpty" => Self::ReconciledGroupEmpty,
            "NonzeroClearingResidual" => Self::NonzeroClearingResidual,
            "PostingAlreadySettled" => Self::PostingAlreadySettled,
            "InvalidGroupTransition" => Self::InvalidGroupTransition,
            "SettlementPostingNotCurrent" => Self::SettlementPostingNotCurrent,
            "DissolveWithoutReversal" => Self::DissolveWithoutReversal,
            "DissolveWithoutRepost" => Self::DissolveWithoutRepost,
            "LegsNotRegrouped" => Self::LegsNotRegrouped,
            "RecipeIncomplete" => Self::RecipeIncomplete,
            "ClaimAccountMismatch" => Self::ClaimAccountMismatch,
            "ClaimSettlementExceedsOpen" => Self::ClaimSettlementExceedsOpen,
            "RefundShapeInvalid" => Self::RefundShapeInvalid,
            "RefundCapExceeded" => Self::RefundCapExceeded,
            "AppendOnlyViolation" => Self::AppendOnlyViolation,
            "CommitSealed" => Self::CommitSealed,
            "RenameTargetMissing" => Self::RenameTargetMissing,
            "UnknownTarget" => Self::UnknownTarget,
            "UnknownRecipe" => Self::UnknownRecipe,
            "SecondEntity" => Self::SecondEntity,
            "ValuationNotAllowed" => Self::ValuationNotAllowed,
            "IdempotencyConflict" => Self::IdempotencyConflict,
            "TransitiveInvalid" => Self::TransitiveInvalid,
            "InvalidId" => Self::InvalidId,
            "DuplicateId" => Self::DuplicateId,
            "InvalidMoney" => Self::InvalidMoney,
            "InvalidCurrency" => Self::InvalidCurrency,
            "InvalidDate" => Self::InvalidDate,
            "InvalidShape" => Self::InvalidShape,
            "StatementNotAgreed" => Self::StatementNotAgreed,
            "CrossEntityGap" => Self::CrossEntityGap,
            "OrphanEvidence" => Self::OrphanEvidence,
            "ForeignKeysDisabled" => Self::ForeignKeysDisabled,
            "UnsealedRows" => Self::UnsealedRows,
            "PartialCommit" => Self::PartialCommit,
            "ValidatorMissed" => Self::ValidatorMissed,
            "DoctorDrift" => Self::DoctorDrift,
            "IntegrityFailed" => Self::IntegrityFailed,
            "ForeignKeyCheckFailed" => Self::ForeignKeyCheckFailed,
            "EvidenceHashMismatch" => Self::EvidenceHashMismatch,
            "UnmappedStorageError" => Self::UnmappedStorageError,
            "StorageFault" => Self::StorageFault,
            "WriteFrozen" => Self::WriteFrozen,
            _ => return None,
        })
    }
}

/// Public error. `token` is set only for `InvalidShape`, and only from §4.3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: Code,
    pub class: Class,
    pub refs: Vec<String>,
    pub ids: Vec<String>,
    pub token: Option<&'static str>,
}

impl Error {
    pub fn new(code: Code, refs: &[&str], ids: Vec<String>) -> Self {
        Self {
            code,
            class: code.class(),
            refs: refs.iter().map(|s| (*s).to_string()).collect(),
            ids,
            token: None,
        }
    }

    pub fn with_token(mut self, token: &'static str) -> Self {
        self.token = Some(token);
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "code={} class={}",
            self.code.as_str(),
            self.class.as_str()
        )?;
        if let Some(token) = self.token {
            write!(f, " token={token}")?;
        }
        if !self.refs.is_empty() {
            write!(f, " refs={}", self.refs.join(","))?;
        }
        if !self.ids.is_empty() {
            write!(f, " ids={}", self.ids.join(","))?;
        }
        Ok(())
    }
}

/// Bugcheck codes that `record_write_freeze` may store, and that persist a freeze.
pub fn persists_freeze(code: Code) -> bool {
    matches!(
        code,
        Code::ValidatorMissed
            | Code::PartialCommit
            | Code::UnmappedStorageError
            | Code::DoctorDrift
            | Code::IntegrityFailed
            | Code::ForeignKeyCheckFailed
            | Code::UnsealedRows
            | Code::EvidenceHashMismatch
    )
}
