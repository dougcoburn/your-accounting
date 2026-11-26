//! Map SQLite text to the closed taxonomy, then drop the text.
//!
//! Substring order follows kernel spec §4.3. The SQL text is not stored.

use rusqlite::Error as SqlError;

use crate::v2::error::{Code, Error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlSite {
    Begin,
    Read,
    Staging,
    /// Insert of the `commits` row, where `commits_seal` runs.
    Seal,
    Finalize {
        commits_inserted: bool,
    },
}

pub(crate) fn map_sql(err: SqlError, site: SqlSite) -> Error {
    if is_busy(&err) {
        return Error::new(Code::WriteLocked, &[], vec![]);
    }
    if matches!(site, SqlSite::Read) {
        return Error::new(Code::StorageFault, &[], vec![]);
    }
    map_message(&err.to_string(), site)
}

fn is_busy(err: &SqlError) -> bool {
    matches!(
        err,
        SqlError::SqliteFailure(ffi, _) if ffi.code == rusqlite::ErrorCode::DatabaseBusy
    )
}

pub(crate) fn map_message(msg: &str, site: SqlSite) -> Error {
    if let Some((code, seal_is_miss)) = map_raise(msg) {
        if matches!(site, SqlSite::Seal) && seal_is_miss {
            return Error::new(Code::ValidatorMissed, &[code.as_str()], vec![]);
        }
        return Error::new(code, refs_for(code), vec![]);
    }
    map_constraint(msg, site)
}

fn refs_for(code: Code) -> &'static [&'static str] {
    match code {
        Code::StaleRevision => &["AT-032"],
        Code::NonLeafCategory | Code::CategoryParentInvalid => &["INV-003"],
        Code::CurrencyMismatch => &["INV-005"],
        Code::EntityMismatch => &["ADR 0004"],
        Code::LedgerKindMismatch => &["ADR 0008"],
        Code::Unbalanced => &["INV-001"],
        Code::InsufficientPostings => &["INV-002"],
        Code::SecondEntity => &["ADR 0004"],
        Code::IdempotencyConflict => &["ADR 0004"],
        Code::AppendOnlyViolation => &["ADR 0002"],
        _ => &[],
    }
}

/// `(code, true)` when a seal RAISE of this text is `ValidatorMissed`.
fn map_raise(msg: &str) -> Option<(Code, bool)> {
    const ROWS: &[(&str, Code, bool)] = &[
        ("AT-032: stale revision", Code::StaleRevision, true),
        (
            "event recorded_at must equal",
            Code::RecordedAtMismatch,
            true,
        ),
        (
            "INV-002: every event needs at least two postings",
            Code::InsufficientPostings,
            true,
        ),
        ("INV-001: event does not balance", Code::Unbalanced, true),
        (
            "reversal does not exactly cancel",
            Code::ReversalDoesNotCancel,
            true,
        ),
        (
            "must be flagged superseded_by",
            Code::SupersedeMissing,
            true,
        ),
        (
            "INV-006: draft groups never hold postings",
            Code::DraftHasPostings,
            true,
        ),
        (
            "INV-008: a reconciled group must hold",
            Code::ReconciledGroupEmpty,
            true,
        ),
        (
            "nonzero clearing residual",
            Code::NonzeroClearingResidual,
            true,
        ),
        (
            "a void must reverse every current event",
            Code::VoidWithoutReversal,
            true,
        ),
        (
            "dissolving a reconciled group must reverse",
            Code::DissolveWithoutReversal,
            true,
        ),
        (
            "observed legs must be reposted",
            Code::DissolveWithoutRepost,
            true,
        ),
        (
            "only superseded_by may be set",
            Code::SupersedeInvalid,
            false,
        ),
        (
            "targets a current non-reversal event and keeps its effective date",
            Code::ReversalTargetInvalid,
            false,
        ),
        (
            "an event is inserted current",
            Code::EventInsertedSuperseded,
            false,
        ),
        ("leaf category", Code::NonLeafCategory, false),
        (
            "category parent must be",
            Code::CategoryParentInvalid,
            false,
        ),
        (
            "ledger kind does not match",
            Code::LedgerKindMismatch,
            false,
        ),
        ("posting currency must match", Code::CurrencyMismatch, false),
        (
            "observation currency must match",
            Code::CurrencyMismatch,
            false,
        ),
        (
            "corroborating row must be",
            Code::CorroborationMismatch,
            false,
        ),
        (
            "posting must match its observation",
            Code::ObservationMismatch,
            false,
        ),
        (
            "voided observation cannot be interpreted",
            Code::VoidedObservation,
            false,
        ),
        (
            "invalid settlement group transition",
            Code::InvalidGroupTransition,
            false,
        ),
        (
            "only current, non-reversal postings",
            Code::SettlementPostingNotCurrent,
            false,
        ),
        (
            "receivable/payable ledger account",
            Code::ClaimAccountMismatch,
            false,
        ),
        (
            "settlement exceeds the open claim",
            Code::ClaimSettlementExceedsOpen,
            false,
        ),
        (
            "opposite-side posting to the purchase",
            Code::RefundShapeInvalid,
            false,
        ),
        (
            "INV-015: refund links exceed",
            Code::RefundCapExceeded,
            false,
        ),
        (
            "valuation snapshots only on non-cash",
            Code::ValuationNotAllowed,
            false,
        ),
        (
            "rename target does not exist",
            Code::RenameTargetMissing,
            false,
        ),
        ("commit already sealed", Code::CommitSealed, false),
        ("are never updated", Code::AppendOnlyViolation, false),
        ("are never deleted", Code::AppendOnlyViolation, false),
        ("is immutable", Code::AppendOnlyViolation, false),
        ("is never deleted", Code::AppendOnlyViolation, false),
        ("are immutable", Code::AppendOnlyViolation, false),
    ];
    ROWS.iter()
        .find(|(needle, _, _)| msg.contains(needle))
        .map(|(_, code, miss)| (*code, *miss))
}

fn map_constraint(msg: &str, site: SqlSite) -> Error {
    if msg.contains("seq = expected_revision + 1") {
        return Error::new(Code::RevisionMismatch, &[], vec![]);
    }
    if msg.contains("UNIQUE constraint failed: entity.singleton") {
        return Error::new(Code::SecondEntity, &["ADR 0004"], vec![]);
    }
    if msg.contains("UNIQUE constraint failed: observations.feed_id, observations.external_id") {
        return Error::new(Code::ImportConflict, &[], vec![]);
    }
    if msg.contains("UNIQUE constraint failed: settlement_postings.posting_id") {
        return Error::new(Code::PostingAlreadySettled, &["ADR 0006"], vec![]);
    }
    if msg.contains("UNIQUE constraint failed: observation_voids.observation_id") {
        return Error::new(Code::ObservationAlreadyVoided, &[], vec![]);
    }
    if msg.contains("UNIQUE constraint failed: cross_entity_outbox.idempotency_key")
        || msg.contains("UNIQUE constraint failed: cross_entity_inbox.idempotency_key")
    {
        return Error::new(Code::IdempotencyConflict, &["ADR 0004"], vec![]);
    }
    if msg.contains("UNIQUE constraint failed: commits.seq") {
        return Error::new(Code::RevisionMismatch, &[], vec![]);
    }
    if msg.contains("UNIQUE constraint failed: accounting_events.reverses_id") {
        return Error::new(Code::ReversalTargetInvalid, &[], vec![]);
    }
    if msg.contains(
        "UNIQUE constraint failed: settlement_group_states.group_id, settlement_group_states.seq",
    ) {
        return Error::new(Code::InvalidGroupTransition, &["ADR 0006"], vec![]);
    }
    if let Some(rest) = msg.split("UNIQUE constraint failed:").nth(1) {
        let id_column = rest.split(',').any(|part| {
            part.trim()
                .rsplit('.')
                .next()
                .is_some_and(|col| col == "id")
        });
        if id_column {
            return Error::new(Code::DuplicateId, &[], vec![]);
        }
        return Error::new(Code::InvalidShape, &[], vec![]).with_token("unique_other");
    }
    if msg.contains("FOREIGN KEY constraint failed") {
        return match site {
            SqlSite::Finalize {
                commits_inserted: false,
            } => Error::new(Code::PartialCommit, &["AT-013"], vec![]),
            SqlSite::Finalize {
                commits_inserted: true,
            }
            | SqlSite::Seal => Error::new(Code::ValidatorMissed, &["UnknownTarget"], vec![]),
            _ => Error::new(Code::UnknownTarget, &[], vec![]),
        };
    }
    if msg.contains("cannot store REAL value in INTEGER column") || msg.contains("amount_minor") {
        return Error::new(Code::InvalidMoney, &[], vec![]);
    }
    if msg.contains("CHECK constraint failed") {
        if msg.contains("length(id) = 26") || msg.contains("[^0-9A-HJKMNP-TV-Z]") {
            return Error::new(Code::InvalidId, &[], vec![]);
        }
        if msg.contains("currency") {
            return Error::new(Code::InvalidCurrency, &[], vec![]);
        }
        if msg.contains("date(") || msg.contains("*Z'") {
            return Error::new(Code::InvalidDate, &[], vec![]);
        }
        if msg.contains("last4") {
            return Error::new(Code::InvalidShape, &[], vec![]).with_token("last4");
        }
        if msg.contains("sha256") || msg.contains("row_hash") {
            return Error::new(Code::InvalidShape, &[], vec![]).with_token("hash");
        }
        if let Some(token) = check_token(msg) {
            return Error::new(Code::InvalidShape, &[], vec![]).with_token(token);
        }
        return Error::new(Code::UnmappedStorageError, &[], vec![]);
    }
    if matches!(site, SqlSite::Begin) {
        return Error::new(Code::StorageFault, &[], vec![]);
    }
    Error::new(Code::UnmappedStorageError, &[], vec![])
}

fn check_token(msg: &str) -> Option<&'static str> {
    if msg.contains("'freeze'") || msg.contains("'cross_entity'") {
        return Some("commit_kind");
    }
    if msg.contains("'extraction_error'") {
        return Some("void_reason");
    }
    if msg.contains("'inflow'") {
        return Some("observation_direction");
    }
    if msg.contains("'forgiveness'") {
        return Some("settlement_kind");
    }
    if msg.contains("'partial_settlement'") {
        return Some("outbox_action");
    }
    if msg.contains("'appraisal'") {
        return Some("valuation_source");
    }
    if msg.contains("'monarch_csv'") {
        return Some("feed_kind");
    }
    if msg.contains("'black_box'") {
        return Some("tracking_mode");
    }
    if msg.contains("'employer'") {
        return Some("party_label");
    }
    if msg.contains("'opening_equity'") {
        return Some("ledger_kind");
    }
    if msg.contains("'gift_card'") || msg.contains("'checking'") {
        return Some("financial_kind");
    }
    if msg.contains("'income'") && msg.contains("'expense'") && msg.contains("'asset'") {
        return Some("account_type");
    }
    if msg.contains("period_start") {
        return Some("period_order");
    }
    if msg.contains("'debit'") && msg.contains("'credit'") {
        return Some("posting_side");
    }
    if msg.contains("'revoke'") {
        return Some("grant_role");
    }
    None
}
