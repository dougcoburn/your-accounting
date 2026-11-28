//! Source identity. A feed that has a stable id keys a movement by that id
//! plus the financial account. Monarch `Id` and a QFX/OFX FITID both use
//! [`FeedIdStrategy`]. A time-range CSV with no id plugs in through
//! [`IdLessStrategy`] later. The weekly cadence and the month-end statement
//! rules are proposed ADR 0012. This unit does not match an id-less source.

use std::fmt;

/// Identity of one movement inside a feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedIdentity {
    pub external_id: String,
    pub financial_account_id: String,
}

pub trait IdentityStrategy {
    fn identify(
        &self,
        external_id: &str,
        financial_account_id: &str,
    ) -> Result<FeedIdentity, IdentityError>;
}

/// Monarch and any other source that already has a stable feed id.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeedIdStrategy;

impl IdentityStrategy for FeedIdStrategy {
    fn identify(
        &self,
        external_id: &str,
        financial_account_id: &str,
    ) -> Result<FeedIdentity, IdentityError> {
        Ok(FeedIdentity {
            external_id: external_id.to_string(),
            financial_account_id: financial_account_id.to_string(),
        })
    }
}

/// Seam for a time-range CSV or statement with no stable id. The follow-up
/// unit (proposed ADR 0012) classifies each row as a confident match, a
/// possible match for review, or new. Calling this strategy does not match
/// and does not write.
#[derive(Debug, Default, Clone, Copy)]
pub struct IdLessStrategy;

impl IdentityStrategy for IdLessStrategy {
    fn identify(
        &self,
        _external_id: &str,
        _financial_account_id: &str,
    ) -> Result<FeedIdentity, IdentityError> {
        Err(IdentityError::IdLessNotInThisUnit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    IdLessNotInThisUnit,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdLessNotInThisUnit => f.write_str("id-less identity is not in this unit"),
        }
    }
}

impl std::error::Error for IdentityError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_id_is_the_external_id_plus_the_account() {
        let key = FeedIdStrategy
            .identify("row-1", "01ARZ3NDEKTSV4RRFFQ69GACCT")
            .expect("key");
        assert_eq!(key.external_id, "row-1");
        assert_eq!(key.financial_account_id, "01ARZ3NDEKTSV4RRFFQ69GACCT");
    }

    #[test]
    fn an_id_less_source_is_not_matched_here() {
        let err = IdLessStrategy
            .identify("row-1", "01ARZ3NDEKTSV4RRFFQ69GACCT")
            .expect_err("seam");
        assert_eq!(err.to_string(), "id-less identity is not in this unit");
        assert!(!err.to_string().contains("row-1"));
    }
}
