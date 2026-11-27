//! Recipe-context storage text for refund links (ADR 0011). The links themselves
//! are staged by the recipes participant, not by this module.

use crate::v2::error::{Code, Error};

pub(crate) fn map_message(msg: &str) -> Option<Error> {
    if msg.contains("opposite-side posting to the purchase") {
        return Some(Error::new(Code::RefundShapeInvalid, &["ADR 0008"], vec![]));
    }
    if msg.contains("INV-015: refund links exceed") {
        return Some(Error::new(Code::RefundCapExceeded, &["INV-015"], vec![]));
    }
    if msg.contains("UNIQUE constraint failed: refund_links.refund_posting_id") {
        return Some(Error::new(Code::InvalidShape, &[], vec![]).with_token("unique_other"));
    }
    None
}
