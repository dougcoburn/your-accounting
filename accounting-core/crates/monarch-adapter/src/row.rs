//! One Monarch data row, turned into an observation fact.
//!
//! The hash covers the raw column text. Hints are stored beside it and are not
//! part of the hash, so a later hint change does not rewrite an observation.

use sha2::{Digest, Sha256};

use crate::error::{column_at, RowFault, COLUMNS};

const HASH_PREFIX: &str = "monarch-row-v1\n";
const UNTAGGED_RULE: &str = "account_owner_or_same_owner_transfer";

#[derive(Debug, Clone)]
pub struct SourceRow {
    pub line: u32,
    pub external_id: String,
    pub occurred_date: String,
    pub direction: &'static str,
    pub amount_minor: i64,
    pub row_hash: String,
    pub description: String,
    pub payload_json: String,
    pub last4: String,
}

pub fn interpret(line: u32, fields: &[String]) -> Result<SourceRow, Vec<RowFault>> {
    if fields.len() != COLUMNS.len() {
        let mut faults = Vec::new();
        if fields.len() < COLUMNS.len() {
            for index in fields.len()..COLUMNS.len() {
                faults.push(RowFault {
                    row: line,
                    column: column_at(index),
                });
            }
        } else {
            faults.push(RowFault {
                row: line,
                column: "Id",
            });
        }
        return Err(faults);
    }
    let mut values: [String; 14] = std::array::from_fn(|_| String::new());
    for (index, value) in fields.iter().enumerate() {
        values[index] = value.clone();
    }
    let mut faults = Vec::new();
    if !valid_date(&values[0]) {
        faults.push(fault(line, "Date"));
    }
    if values[2].is_empty() || values[2].contains(['\n', '\r']) {
        faults.push(fault(line, "Category"));
    }
    let last4 = match account_last4(&values[3]) {
        Some(last4) => last4,
        None => {
            faults.push(fault(line, "Account"));
            String::new()
        }
    };
    let amount = match parse_amount(&values[6]) {
        Some(amount) => amount,
        None => {
            faults.push(fault(line, "Amount"));
            (true, 1)
        }
    };
    let reviewed = match parse_bool(&values[9]) {
        Some(value) => value,
        None => {
            faults.push(fault(line, "Reviewed"));
            false
        }
    };
    let pending = match parse_bool(&values[12]) {
        Some(value) => value,
        None => {
            faults.push(fault(line, "Pending"));
            false
        }
    };
    if values[13].is_empty() || values[13].contains(['\n', '\r', '\0']) {
        faults.push(fault(line, "Id"));
    }
    if !faults.is_empty() {
        return Err(faults);
    }
    let (negative, minor) = amount;
    let direction = if negative { "outflow" } else { "inflow" };
    let e_tags = entity_tags(&values[7]);
    let payload_json = payload(&values, direction, minor, reviewed, pending, &e_tags);
    Ok(SourceRow {
        line,
        row_hash: row_hash(&values),
        description: values[1].clone(),
        external_id: values[13].clone(),
        occurred_date: values[0].clone(),
        last4,
        direction,
        amount_minor: minor,
        payload_json,
    })
}

fn fault(row: u32, column: &'static str) -> RowFault {
    RowFault { row, column }
}

fn row_hash(values: &[String; 14]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(HASH_PREFIX.as_bytes());
    for (name, value) in COLUMNS.iter().zip(values.iter()) {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }
    hex_encode(&hasher.finalize())
}

fn payload(
    values: &[String; 14],
    direction: &str,
    minor: i64,
    reviewed: bool,
    pending: bool,
    e_tags: &[String],
) -> String {
    let mut out = String::from("{\"v\":1,\"columns\":{");
    for (index, name) in COLUMNS.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_str(&mut out, name);
        out.push(':');
        push_json_str(&mut out, &values[index]);
    }
    out.push_str("},\"direction\":");
    push_json_str(&mut out, direction);
    out.push_str(",\"amount_minor\":");
    out.push_str(&minor.to_string());
    out.push_str(",\"reviewed_hint\":");
    out.push_str(if reviewed { "true" } else { "false" });
    out.push_str(",\"reviewed_authoritative\":false,\"pending_hint\":");
    out.push_str(if pending { "true" } else { "false" });
    out.push_str(",\"pending_authoritative\":false,\"e_tag_hints\":[");
    for (index, tag) in e_tags.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_str(&mut out, tag);
    }
    out.push_str("],\"untagged_rule\":");
    if e_tags.is_empty() {
        push_json_str(&mut out, UNTAGGED_RULE);
    } else {
        out.push_str("null");
    }
    out.push('}');
    out
}

fn push_json_str(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn entity_tags(raw: &str) -> Vec<String> {
    let mut tags: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|tag| {
            tag.starts_with("e-") && tag.len() > 2 && !tag[2..].chars().any(char::is_whitespace)
        })
        .map(str::to_string)
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" | "TRUE" | "True" => Some(true),
        "false" | "FALSE" | "False" => Some(false),
        _ => None,
    }
}

/// Signed minor units for a 2-decimal currency. `None` when the text is not that shape.
pub fn parse_amount(amount: &str) -> Option<(bool, i64)> {
    let (negative, rest) = match amount.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, amount),
    };
    let (dollars, cents) = rest.split_once('.')?;
    if dollars.is_empty()
        || !dollars.bytes().all(|byte| byte.is_ascii_digit())
        || cents.len() != 2
        || !cents.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let dollars: i64 = dollars.parse().ok()?;
    let cents: i64 = cents.parse().ok()?;
    let minor = dollars.checked_mul(100)?.checked_add(cents)?;
    if minor == 0 {
        return None;
    }
    Some((negative, minor))
}

fn account_last4(account: &str) -> Option<String> {
    let bytes = account.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    let tail = &bytes[bytes.len() - 4..];
    if !tail.iter().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if bytes.len() > 4 && bytes[bytes.len() - 5].is_ascii_digit() {
        return None;
    }
    std::str::from_utf8(tail).ok().map(str::to_string)
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let year: i32 = value[0..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) {
        return false;
    }
    (1..=days_in_month(year, month)).contains(&day)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(amount: &str, pending: &str) -> Vec<String> {
        vec![
            "2025-10-02",
            "Northwind Market",
            "Groceries",
            "Checking ...1234",
            "POS NORTHWIND",
            "",
            amount,
            "e-household",
            "Casey Sample",
            "true",
            "2025-10-02 10:00:00",
            "Rowan Checker",
            pending,
            "nw-01",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }

    #[test]
    fn sign_becomes_direction_and_a_positive_minor_amount() {
        let outflow = interpret(3, &fields("-12.50", "false")).expect("out");
        assert_eq!(outflow.direction, "outflow");
        assert_eq!(outflow.amount_minor, 1250);
        assert_eq!(outflow.last4, "1234");
        assert!(outflow
            .payload_json
            .contains("\"reviewed_authoritative\":false"));
        assert!(outflow
            .payload_json
            .contains("\"pending_authoritative\":false"));
        assert!(outflow
            .payload_json
            .contains("\"e_tag_hints\":[\"e-household\"]"));
        assert!(outflow.payload_json.contains("\"untagged_rule\":null"));

        let inflow = interpret(3, &fields("12.50", "false")).expect("in");
        assert_eq!(inflow.direction, "inflow");
        assert_ne!(outflow.row_hash, inflow.row_hash);
    }

    #[test]
    fn untagged_row_records_the_owner_rule() {
        let mut row = fields("-1.00", "false");
        row[7].clear();
        let source = interpret(4, &row).expect("row");
        assert!(source
            .payload_json
            .contains("\"untagged_rule\":\"account_owner_or_same_owner_transfer\""));
        assert!(source.payload_json.contains("\"e_tag_hints\":[]"));
    }

    #[test]
    fn zero_and_long_account_numbers_name_the_column() {
        let zero = interpret(5, &fields("0.00", "false")).expect_err("zero");
        assert_eq!(zero, vec![fault(5, "Amount")]);
        let mut wide = fields("-1.00", "false");
        wide[3] = "Checking 12345".to_string();
        let account = interpret(6, &wide).expect_err("account");
        assert!(account.iter().any(|item| item.column == "Account"));
        let shown = format!("row={} column={}", account[0].row, account[0].column);
        assert!(!shown.contains("12345"));
        assert!(!shown.contains("Northwind"));
    }

    #[test]
    fn a_changed_amount_changes_the_raw_hash() {
        let first = interpret(3, &fields("-12.50", "false")).expect("first");
        let second = interpret(9, &fields("-13.50", "false")).expect("second");
        assert_ne!(first.row_hash, second.row_hash);
        assert!(second.payload_json.contains("\"pending_hint\":false"));
    }
}
