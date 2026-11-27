//! Application tag lookups and the `tags.name` uniqueness token (ADR 0011).
//! The kernel reference API no longer names tags.

use crate::v2::error::{Code, Error};

#[allow(dead_code)]
pub(crate) const fn tag_name_sql() -> &'static str {
    "SELECT name FROM tags WHERE id = ?1"
}

#[allow(dead_code)]
pub(crate) const fn tag_exists_sql() -> &'static str {
    "SELECT count(*) FROM tags WHERE id = ?1"
}

pub(crate) fn map_message(msg: &str) -> Option<Error> {
    if msg.contains("UNIQUE constraint failed: tags.name") {
        return Some(Error::new(Code::InvalidShape, &[], vec![]).with_token("unique_other"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_lookups_name_the_tags_table() {
        assert!(tag_name_sql().contains("tags"));
        assert!(tag_exists_sql().contains("tags"));
        let err = map_message("UNIQUE constraint failed: tags.name").expect("unique");
        assert_eq!(err.token, Some("unique_other"));
    }
}
