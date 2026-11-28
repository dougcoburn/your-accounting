//! Monarch CSV records. A fault names the starting line and the column index only.

use crate::error::{column_at, ImportError, RowFault};

#[derive(Debug, PartialEq, Eq)]
pub struct RawRecord {
    pub line: u32,
    pub fields: Vec<String>,
}

pub fn parse_records(text: &str) -> Result<Vec<RawRecord>, ImportError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1u32;
    let mut record_line = 1u32;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                continue;
            }
            return Err(fault(record_line, fields.len()));
        }
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                if ch == '\n' {
                    line = line.saturating_add(1);
                }
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' => {
                if !field.is_empty() {
                    return Err(fault(record_line, fields.len()));
                }
                in_quotes = true;
            }
            ',' => {
                fields.push(std::mem::take(&mut field));
            }
            '\n' => {
                fields.push(std::mem::take(&mut field));
                push_record(&mut records, record_line, &mut fields);
                line = line.saturating_add(1);
                record_line = line;
            }
            _ => field.push(ch),
        }
    }
    if in_quotes {
        return Err(fault(record_line, fields.len()));
    }
    if !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        push_record(&mut records, record_line, &mut fields);
    }
    Ok(records)
}

fn push_record(records: &mut Vec<RawRecord>, line: u32, fields: &mut Vec<String>) {
    if fields.iter().all(|field| field.is_empty()) {
        fields.clear();
        return;
    }
    records.push(RawRecord {
        line,
        fields: std::mem::take(fields),
    });
}

fn fault(row: u32, index: usize) -> ImportError {
    ImportError::rows(vec![RowFault {
        row,
        column: column_at(index),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_commas_and_newlines_stay_in_the_field() {
        let text = "\
# synthetic
Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id
2025-10-08,Pine Market,Home,Checking ...1234,\"POS ONE
SAMPLE\",,\"-1.00\",e-household,Casey Sample,true,2025-10-08 08:00:00,Rowan Checker,false,row-1
";
        let records = parse_records(text).expect("parse");
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].line, 2);
        assert_eq!(records[2].line, 3);
        assert_eq!(records[2].fields[4], "POS ONE\nSAMPLE");
        assert_eq!(records[2].fields[13], "row-1");
    }

    #[test]
    fn unterminated_quote_names_the_column_only() {
        let text = "Date,Merchant\n\"open\n";
        let err = parse_records(text).expect_err("quote");
        let shown = err.to_string();
        assert_eq!(shown, "row=2 column=Date");
        assert!(!shown.contains("open"));
    }
}
