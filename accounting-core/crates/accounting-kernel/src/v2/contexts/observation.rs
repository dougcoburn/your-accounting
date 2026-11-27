//! Observation-import storage text (ADR 0011). The store itself is the import context.

use crate::v2::error::{Code, Error};

#[allow(dead_code)]
pub(crate) const fn feed_name_sql() -> &'static str {
    "SELECT name FROM feeds WHERE id = ?1"
}

#[allow(dead_code)]
pub(crate) const fn feed_exists_sql() -> &'static str {
    "SELECT count(*) FROM feeds WHERE id = ?1"
}

pub(crate) fn map_message(msg: &str) -> Option<Error> {
    if let Some((code, refs)) = raise(msg) {
        return Some(Error::new(code, refs, vec![]));
    }
    if msg.contains("UNIQUE constraint failed: observations.feed_id, observations.external_id") {
        return Some(Error::new(Code::ImportConflict, &[], vec![]));
    }
    if msg.contains("UNIQUE constraint failed: observation_voids.observation_id") {
        return Some(Error::new(Code::ObservationAlreadyVoided, &[], vec![]));
    }
    if msg.contains("UNIQUE constraint failed: evidence_deletions.sha256") {
        return Some(Error::new(Code::InvalidShape, &[], vec![]).with_token("unique_other"));
    }
    if msg.contains("CHECK constraint failed") {
        if msg.contains("sha256") || msg.contains("row_hash") {
            return Some(Error::new(Code::InvalidShape, &[], vec![]).with_token("hash"));
        }
        if let Some(token) = check_token(msg) {
            return Some(Error::new(Code::InvalidShape, &[], vec![]).with_token(token));
        }
    }
    None
}

fn raise(msg: &str) -> Option<(Code, &'static [&'static str])> {
    const ROWS: &[(&str, Code, &[&str])] = &[
        (
            "observation currency must match",
            Code::CurrencyMismatch,
            &["INV-005"],
        ),
        (
            "corroborating row must be",
            Code::CorroborationMismatch,
            &[],
        ),
        (
            "posting must match its observation",
            Code::ObservationMismatch,
            &["INV-007"],
        ),
        (
            "voided observation cannot be interpreted",
            Code::VoidedObservation,
            &["ADR 0002"],
        ),
        (
            "observed legs must be reposted",
            Code::DissolveWithoutRepost,
            &["ADR 0006"],
        ),
    ];
    ROWS.iter()
        .find(|(needle, _, _)| msg.contains(needle))
        .map(|(_, code, refs)| (*code, *refs))
}

fn check_token(msg: &str) -> Option<&'static str> {
    if msg.contains("'extraction_error'") {
        return Some("void_reason");
    }
    if msg.contains("'inflow'") {
        return Some("observation_direction");
    }
    if msg.contains("'monarch_csv'") {
        return Some("feed_kind");
    }
    if msg.contains("event_id IS NOT NULL") && msg.contains("observation_id IS NOT NULL") {
        return Some("evidence_link_target");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::error::Class;
    use crate::v2::participant::Subject;
    use crate::v2::sqlmap::{map_message as kernel_map, SqlSite};

    #[test]
    fn observation_subject_is_context_supplied() {
        let subject = Subject::Observation {
            feed_id: "feed".to_string(),
            external_id: "ext".to_string(),
            commit_id: "01ARZ3NDEKTSV4RRFFQ69GCMT1".to_string(),
            seq: 2,
        };
        match subject {
            Subject::Observation { seq, .. } => assert_eq!(seq, 2),
            Subject::RecipeInstance { .. } => panic!("observation subject"),
        }
    }

    #[test]
    fn observation_text_maps_here_and_through_the_kernel_fallback() {
        let conflict =
            map_message("UNIQUE constraint failed: observations.feed_id, observations.external_id")
                .expect("import");
        assert_eq!(conflict.code, Code::ImportConflict);
        assert_eq!(conflict.class, Class::Import);

        let hashed = map_message("CHECK constraint failed: length(row_hash) = 64").expect("hash");
        assert_eq!(hashed.token, Some("hash"));

        let kind = map_message("CHECK constraint failed: kind IN ('monarch_csv','bankstmt_json')")
            .expect("kind");
        assert_eq!(kind.token, Some("feed_kind"));

        let voided = kernel_map(
            "ADR 0002: a voided observation cannot be interpreted",
            SqlSite::Staging,
        );
        assert_eq!(voided.code, Code::VoidedObservation);

        let missed = kernel_map(
            "ADR 0006: a dissolved group's observed legs must be reposted in the same commit",
            SqlSite::Seal,
        );
        assert_eq!(missed.code, Code::ValidatorMissed);
        assert_eq!(
            missed.refs,
            vec!["DissolveWithoutRepost".to_string(), "ADR 0011".to_string()]
        );

        assert!(feed_name_sql().contains("feeds"));
        assert!(feed_exists_sql().contains("feeds"));
    }
}
