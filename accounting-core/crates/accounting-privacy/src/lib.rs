//! Redacting logger for transaction processing.
//!
//! A log line may keep the transaction id. Payee, description, and memo are
//! transaction text and are written as `[redacted]`. Tests use Acme Corp as
//! the employer placeholder.

use std::fmt;

/// One transaction being processed. Fields other than `id` are transaction text.
#[derive(Clone, PartialEq, Eq)]
pub struct TransactionRecord {
    pub id: String,
    pub payee: String,
    pub description: String,
    pub memo: String,
}

impl fmt::Debug for TransactionRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransactionRecord")
            .field("id", &self.id)
            .field("payee", &"[redacted]")
            .field("description", &"[redacted]")
            .field("memo", &"[redacted]")
            .finish()
    }
}

/// Log that has already dropped transaction text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RedactingLogger {
    lines: Vec<String>,
}

impl RedactingLogger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `txn` was processed. Payee, description, and memo are not written.
    pub fn record(&mut self, txn: &TransactionRecord) {
        let id = &txn.id;
        self.lines.push(format!(
            "processed id={id} payee=[redacted] description=[redacted] memo=[redacted]"
        ));
    }

    pub fn output(&self) -> String {
        self.lines.join("\n")
    }
}

/// Result of processing one transaction. The id is kept; transaction text is not.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutcome {
    pub id: String,
}

/// Process one transaction and append a redacted log line.
pub fn process_transaction(txn: &TransactionRecord, log: &mut RedactingLogger) -> ProcessOutcome {
    log.record(txn);
    ProcessOutcome { id: txn.id.clone() }
}

/// True when `log` still contains payee, description, or memo text.
#[must_use]
pub fn transaction_text_leaked(log: &str, txn: &TransactionRecord) -> bool {
    let haystack = log.to_ascii_lowercase();
    [
        txn.payee.as_str(),
        txn.description.as_str(),
        txn.memo.as_str(),
    ]
    .into_iter()
    .any(|field| {
        let needle = field.trim();
        !needle.is_empty() && haystack.contains(&needle.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::{process_transaction, transaction_text_leaked, RedactingLogger, TransactionRecord};

    fn sample_transaction() -> TransactionRecord {
        TransactionRecord {
            id: "s-pay-01".to_string(),
            payee: "Acme Corp".to_string(),
            description: "October synthetic paycheck".to_string(),
            memo: "partial reimbursement remainder stays open".to_string(),
        }
    }

    #[test]
    fn clean_log_redacts_synthetic_transaction() {
        let txn = sample_transaction();
        let mut log = RedactingLogger::new();
        let outcome = process_transaction(&txn, &mut log);
        let output = log.output();

        assert_eq!(outcome.id, txn.id);
        assert!(!transaction_text_leaked(&output, &txn));
        assert!(output.contains("id=s-pay-01"));
        assert!(output.contains("payee=[redacted]"));
        assert!(output.contains("description=[redacted]"));
        assert!(output.contains("memo=[redacted]"));
        assert!(!output.contains("Acme"));
        assert!(!output.contains("paycheck"));
        assert!(!output.contains("reimbursement"));

        let debug = format!("{txn:?}");
        assert!(!transaction_text_leaked(&debug, &txn));
        assert!(debug.contains("s-pay-01"));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn planted_unredacted_log_fails_the_guard() {
        let txn = sample_transaction();
        let planted = format!(
            "processed id={} payee={} description={} memo={}",
            txn.id, txn.payee, txn.description, txn.memo
        );
        assert!(
            transaction_text_leaked(&planted, &txn),
            "guard should fail when a planted log keeps payee and description"
        );
        assert!(
            transaction_text_leaked(&planted.to_ascii_uppercase(), &txn),
            "guard should fail when the planted log changes case"
        );

        let mut log = RedactingLogger::new();
        let outcome = process_transaction(&txn, &mut log);
        assert_eq!(outcome.id, txn.id);
        assert!(!transaction_text_leaked(&log.output(), &txn));
    }
}
