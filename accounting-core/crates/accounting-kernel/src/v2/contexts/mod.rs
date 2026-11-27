//! Context-owned storage mappings (ADR 0011).
//!
//! The kernel mapper calls these when a SQLite message belongs to a context
//! table. Observation import, recipes, and tags each own their rows.

mod import;
mod observation;
mod recipes;
mod tags;

pub use import::{
    observation_links, EvidenceRecord, EvidenceTarget, ImportBatch, ImportOutcome, NewEvidence,
    NewFeed, ObservationImport, ObservationInput, ObservationLink, CHECK_DOUBLE_CONSUMED,
    CHECK_EVIDENCE_HASH, CHECK_ORPHAN_EVIDENCE, CHECK_UNSEALED,
};

use rusqlite::Error as SqlError;

use crate::v2::error::{Code, Error};
use crate::v2::sqlmap::SqlSite;

pub(crate) fn map_owned(msg: &str, site: SqlSite) -> Option<Error> {
    if matches!(site, SqlSite::Seal) {
        if let Some(err) = seal_backstop(msg) {
            return Some(err);
        }
    }
    observation::map_message(msg)
        .or_else(|| recipes::map_message(msg))
        .or_else(|| tags::map_message(msg))
}

/// A `commits_seal` rule the participant's `validate` was required to reject.
pub(crate) fn seal_backstop(msg: &str) -> Option<Error> {
    let rule = if msg.contains("a void must reverse every current event") {
        "VoidWithoutReversal"
    } else if msg.contains("observed legs must be reposted") {
        "DissolveWithoutRepost"
    } else {
        return None;
    };
    Some(Error::new(
        Code::ValidatorMissed,
        &[rule, "ADR 0011"],
        vec![],
    ))
}

pub(crate) fn map_context_sql(err: SqlError) -> Error {
    if is_auth(&err) {
        return Error::new(Code::UnmappedStorageError, &["ADR 0011"], vec![]);
    }
    let msg = err.to_string();
    if let Some(mapped) = map_owned(&msg, SqlSite::Staging) {
        return mapped;
    }
    crate::v2::sqlmap::map_sql(err, SqlSite::Staging)
}

fn is_auth(err: &SqlError) -> bool {
    err.to_string()
        .to_ascii_lowercase()
        .contains("not authorized")
}
