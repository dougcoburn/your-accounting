//! Ledger-core doctor checks. Kernel tables only.
//!
//! Context tables (observations, evidence, refund links, tags) are not read
//! here. Those contexts register their own checks. Settlement, obligation, and
//! assertion tables stay, because ADR 0011 still calls them kernel-owned.

use rusqlite::Connection;

use crate::v2::error::{Code, Error};
use crate::v2::sqlmap::{map_sql, SqlSite};
use crate::v2::types::is_ulid;

use super::{DoctorCheck, Finding, Scope, Severity};

pub const CHECK_INTEGRITY: &str = "ledger.integrity";
pub const CHECK_FOREIGN_KEYS: &str = "ledger.foreign_keys";
pub const CHECK_UNSEALED: &str = "ledger.unsealed_rows";
pub const CHECK_UNBALANCED: &str = "ledger.unbalanced_events";
pub const CHECK_SUPERSEDED_DRIFT: &str = "ledger.superseded_drift";
pub const CHECK_FREEZE: &str = "ledger.freeze";

const CONTEXT: &str = "ledger";

/// Commit-scoped tables ADR 0011 names as kernel-owned. Reference tables with
/// no `commit_id` (`entity`, `parties`, `ledger_accounts`, and the rest) are
/// not in `doctor_unsealed_rows`. `user_grants` is not in that kernel-owned
/// list, so it is not here. The view's observation, evidence, tag, and refund
/// rows are context-owned (ADR 0010 §6.2, ADR 0011).
const UNSEALED_SQL: &str = "
SELECT table_name, row_id, commit_id FROM doctor_unsealed_rows
WHERE table_name IN (
    'accounting_events',
    'event_counterpart_expectations',
    'journal_postings',
    'kernel_state',
    'valuation_snapshots',
    'settlement_groups',
    'settlement_group_states',
    'settlement_postings',
    'settlement_draft_participants',
    'obligations',
    'obligation_settlements',
    'obligation_postings',
    'statements',
    'balance_assertions'
)
ORDER BY table_name, row_id
";

pub(super) fn checks() -> Vec<Box<dyn DoctorCheck>> {
    vec![
        Box::new(IntegrityCheck),
        Box::new(ForeignKeyCheck),
        Box::new(UnsealedRowsCheck),
        Box::new(UnbalancedEventsCheck),
        Box::new(SupersededDriftCheck),
        Box::new(FreezeCheck),
    ]
}

struct IntegrityCheck;
struct ForeignKeyCheck;
struct UnsealedRowsCheck;
struct UnbalancedEventsCheck;
struct SupersededDriftCheck;
struct FreezeCheck;

impl DoctorCheck for IntegrityCheck {
    fn id(&self) -> &'static str {
        CHECK_INTEGRITY
    }
    fn context(&self) -> &'static str {
        CONTEXT
    }
    fn severity(&self) -> Severity {
        Severity::Bugcheck
    }
    fn scope(&self) -> Scope {
        Scope::SingleDb
    }
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        // A corrupt page can fail the pragma with a SQLite error instead of a
        // diagnostic row. That is still IntegrityFailed, and the finding stays
        // quiet: no engine text.
        let failed = bugcheck(CHECK_INTEGRITY, Code::IntegrityFailed, &[], vec![]);
        let mut stmt = match db.prepare("PRAGMA integrity_check") {
            Ok(stmt) => stmt,
            Err(_) => return Ok(vec![failed]),
        };
        let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(rows) => rows,
            Err(_) => return Ok(vec![failed]),
        };
        let mut messages = Vec::new();
        for row in rows {
            match row {
                Ok(message) => messages.push(message),
                Err(_) => return Ok(vec![failed]),
            }
        }
        if messages.as_slice() == ["ok"] {
            return Ok(Vec::new());
        }
        Ok(vec![failed])
    }
}

impl DoctorCheck for ForeignKeyCheck {
    fn id(&self) -> &'static str {
        CHECK_FOREIGN_KEYS
    }
    fn context(&self) -> &'static str {
        CONTEXT
    }
    fn severity(&self) -> Severity {
        Severity::Bugcheck
    }
    fn scope(&self) -> Scope {
        Scope::SingleDb
    }
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        let failed = bugcheck(CHECK_FOREIGN_KEYS, Code::ForeignKeyCheckFailed, &[], vec![]);
        let mut stmt = match db.prepare("PRAGMA foreign_key_check") {
            Ok(stmt) => stmt,
            Err(_) => return Ok(vec![failed]),
        };
        let rows = match stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return Ok(vec![failed]),
        };
        let mut ids = Vec::new();
        let mut any = false;
        for row in rows {
            let (table, rowid) = match row {
                Ok(pair) => pair,
                Err(_) => return Ok(vec![failed]),
            };
            any = true;
            match subject_id(db, &table, rowid) {
                Ok(Some(id)) => push_ulid(&mut ids, id),
                Ok(None) => {}
                Err(_) => return Ok(vec![failed]),
            }
        }
        if !any {
            return Ok(Vec::new());
        }
        Ok(vec![bugcheck(
            CHECK_FOREIGN_KEYS,
            Code::ForeignKeyCheckFailed,
            &["PR-19"],
            ids,
        )])
    }
}

impl DoctorCheck for UnsealedRowsCheck {
    fn id(&self) -> &'static str {
        CHECK_UNSEALED
    }
    fn context(&self) -> &'static str {
        CONTEXT
    }
    fn severity(&self) -> Severity {
        Severity::Bugcheck
    }
    fn scope(&self) -> Scope {
        Scope::SingleDb
    }
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        let mut stmt = db.prepare(UNSEALED_SQL).map_err(read_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(read_err)?;
        let mut ids = Vec::new();
        let mut any = false;
        for row in rows {
            let (table, rowid, commit_id) = row.map_err(read_err)?;
            any = true;
            if let Some(id) = subject_id(db, &table, rowid)? {
                push_ulid(&mut ids, id);
            }
            push_ulid(&mut ids, commit_id);
        }
        if !any {
            return Ok(Vec::new());
        }
        Ok(vec![bugcheck(
            CHECK_UNSEALED,
            Code::UnsealedRows,
            &["PR-19"],
            ids,
        )])
    }
}

impl DoctorCheck for UnbalancedEventsCheck {
    fn id(&self) -> &'static str {
        CHECK_UNBALANCED
    }
    fn context(&self) -> &'static str {
        CONTEXT
    }
    fn severity(&self) -> Severity {
        Severity::Bugcheck
    }
    fn scope(&self) -> Scope {
        Scope::SingleDb
    }
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        let mut stmt = db
            .prepare("SELECT event_id FROM doctor_unbalanced_events ORDER BY event_id, currency")
            .map_err(read_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(read_err)?;
        let mut ids = Vec::new();
        for row in rows {
            push_ulid(&mut ids, row.map_err(read_err)?);
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![bugcheck(
            CHECK_UNBALANCED,
            Code::DoctorDrift,
            &["INV-001"],
            ids,
        )])
    }
}

impl DoctorCheck for SupersededDriftCheck {
    fn id(&self) -> &'static str {
        CHECK_SUPERSEDED_DRIFT
    }
    fn context(&self) -> &'static str {
        CONTEXT
    }
    fn severity(&self) -> Severity {
        Severity::Bugcheck
    }
    fn scope(&self) -> Scope {
        Scope::SingleDb
    }
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        let mut stmt = db
            .prepare("SELECT id FROM doctor_superseded_drift ORDER BY id")
            .map_err(read_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(read_err)?;
        let mut ids = Vec::new();
        for row in rows {
            push_ulid(&mut ids, row.map_err(read_err)?);
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![bugcheck(
            CHECK_SUPERSEDED_DRIFT,
            Code::DoctorDrift,
            &["ADR 0002"],
            ids,
        )])
    }
}

impl DoctorCheck for FreezeCheck {
    fn id(&self) -> &'static str {
        CHECK_FREEZE
    }
    fn context(&self) -> &'static str {
        CONTEXT
    }
    fn severity(&self) -> Severity {
        Severity::Bugcheck
    }
    fn scope(&self) -> Scope {
        Scope::SingleDb
    }
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        let row = db
            .query_row(
                "SELECT k.id, k.active, k.code, k.commit_id
                 FROM kernel_state k
                 JOIN commits c ON c.id = k.commit_id
                 WHERE k.state_key = 'freeze'
                 ORDER BY c.seq DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional_or_none()?;
        let Some((id, active, code, commit_id)) = row else {
            return Ok(Vec::new());
        };
        if active != 1 {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        push_ulid(&mut ids, id);
        push_ulid(&mut ids, commit_id);
        let stored: &str = &code;
        Ok(vec![bugcheck(
            CHECK_FREEZE,
            Code::WriteFrozen,
            &[stored],
            ids,
        )])
    }
}

fn bugcheck(check_id: &'static str, code: Code, refs: &[&str], ids: Vec<String>) -> Finding {
    Finding {
        check_id,
        severity: Severity::Bugcheck,
        code,
        class: code.class(),
        refs: refs.iter().map(|s| (*s).to_string()).collect(),
        ids,
    }
}

fn push_ulid(ids: &mut Vec<String>, id: String) {
    if is_ulid(&id) && !ids.contains(&id) {
        ids.push(id);
    }
}

fn read_err(err: rusqlite::Error) -> Error {
    map_sql(err, SqlSite::Read)
}

fn subject_id(db: &Connection, table: &str, rowid: i64) -> Result<Option<String>, Error> {
    let Some(sql) = subject_sql(table) else {
        return Ok(None);
    };
    db.query_row(sql, [rowid], |row| row.get::<_, String>(0))
        .optional_or_none()
}

fn subject_sql(table: &str) -> Option<&'static str> {
    Some(match table {
        "user_grants" => "SELECT id FROM user_grants WHERE rowid = ?1",
        "accounting_events" => "SELECT id FROM accounting_events WHERE rowid = ?1",
        "settlement_groups" => "SELECT id FROM settlement_groups WHERE rowid = ?1",
        "settlement_group_states" => "SELECT id FROM settlement_group_states WHERE rowid = ?1",
        "settlement_postings" => "SELECT posting_id FROM settlement_postings WHERE rowid = ?1",
        "obligations" => "SELECT id FROM obligations WHERE rowid = ?1",
        "obligation_settlements" => "SELECT id FROM obligation_settlements WHERE rowid = ?1",
        "statements" => "SELECT id FROM statements WHERE rowid = ?1",
        "balance_assertions" => "SELECT id FROM balance_assertions WHERE rowid = ?1",
        "valuation_snapshots" => "SELECT id FROM valuation_snapshots WHERE rowid = ?1",
        "kernel_state" => "SELECT id FROM kernel_state WHERE rowid = ?1",
        "journal_postings" => "SELECT id FROM journal_postings WHERE rowid = ?1",
        "event_counterpart_expectations" => {
            "SELECT event_id FROM event_counterpart_expectations WHERE rowid = ?1"
        }
        "obligation_postings" => "SELECT obligation_id FROM obligation_postings WHERE rowid = ?1",
        "settlement_draft_participants" => {
            "SELECT id FROM settlement_draft_participants WHERE rowid = ?1"
        }
        "party_aliases" => "SELECT party_id FROM party_aliases WHERE rowid = ?1",
        "parties" => "SELECT id FROM parties WHERE rowid = ?1",
        "ledger_accounts" => "SELECT id FROM ledger_accounts WHERE rowid = ?1",
        "financial_accounts" => "SELECT id FROM financial_accounts WHERE rowid = ?1",
        "evidence_deletions" => "SELECT id FROM evidence_deletions WHERE rowid = ?1",
        "observations" => "SELECT id FROM observations WHERE rowid = ?1",
        "refund_links" => "SELECT id FROM refund_links WHERE rowid = ?1",
        "posting_tags" => "SELECT tag_id FROM posting_tags WHERE rowid = ?1",
        _ => return None,
    })
}

trait OptionalOrNone<T> {
    fn optional_or_none(self) -> Result<Option<T>, Error>;
}

impl<T> OptionalOrNone<T> for Result<T, rusqlite::Error> {
    fn optional_or_none(self) -> Result<Option<T>, Error> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(read_err(err)),
        }
    }
}
