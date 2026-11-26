//! SQLite adapter for the v2 write protocol and U17 reference data.
//!
//! Reference rows are not commit-scoped (SPEC-v2-delta §5 Q5). Each successful
//! write still seals a `commits` row so the caller-supplied revision is checked
//! under `BEGIN IMMEDIATE` (ADR 0007). An identical inbox retry that stages
//! nothing else does not seal a commit (spec §2.2).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::v2::error::{persists_freeze, Class, Code, Error};
use crate::v2::sqlmap::{map_sql, SqlSite};
use crate::v2::types::{
    is_ulid, ledger_matches_financial, require_currency, require_name, require_positive,
    require_ulid, shape, AccountType, Clock, FinancialKind, IdSource, InboxAction, LedgerKind,
    PartyLabel, ReferenceKind, Side, TrackingMode,
};

const SCHEMA_SQL: &str = include_str!("../../../../../docs/schema-v2.sql");

/// One already-stored subject named by an idempotent no-op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Observation {
        feed_id: String,
        external_id: String,
        commit_id: String,
        seq: i64,
    },
    IdempotencyKey {
        idempotency_key: String,
        commit_id: String,
        seq: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    pub noop: bool,
    /// Set when this call sealed a commit.
    pub commit_id: Option<String>,
    /// New `seq` when `noop` is false. Unchanged revision when `noop` is true.
    pub seq: i64,
    pub subjects: Vec<Subject>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub id: String,
    pub expected_revision: i64,
    pub seq: i64,
    pub kind: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezeOutcome {
    pub row_id: String,
    pub commit_id: String,
    pub seq: i64,
    pub code: String,
    pub already_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRow {
    pub id: String,
    pub party_id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyRow {
    pub id: String,
    pub name: String,
    pub label: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinancialAccountRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub currency: String,
    pub tracking_mode: String,
    pub last4: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerAccountRow {
    pub id: String,
    pub financial_account_id: Option<String>,
    pub account_type: String,
    pub kind: String,
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxRow {
    pub idempotency_key: String,
    pub recipe_instance_id: String,
    pub source_party_id: String,
    pub action: String,
    pub amount_minor: i64,
    pub currency: String,
    pub event_id: String,
    pub commit_id: String,
}

#[derive(Debug, Clone)]
pub struct NewEntity {
    pub id: Option<String>,
    pub party_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct NewParty {
    pub id: Option<String>,
    pub name: String,
    pub label: PartyLabel,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewFinancialAccount {
    pub id: Option<String>,
    pub name: String,
    pub kind: FinancialKind,
    pub currency: String,
    pub tracking_mode: TrackingMode,
    pub institution: Option<String>,
    pub last4: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewLedgerAccount {
    pub id: Option<String>,
    pub financial_account_id: Option<String>,
    pub account_type: AccountType,
    pub kind: LedgerKind,
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct NewPosting {
    pub id: Option<String>,
    pub ledger_account_id: String,
    pub currency: String,
    pub side: Side,
    pub amount_minor: i64,
    pub party_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewEvent {
    pub id: Option<String>,
    pub recipe_id: String,
    pub recipe_version: String,
    pub effective_date: String,
    pub postings: Vec<NewPosting>,
}

#[derive(Debug, Clone)]
pub struct InboxApply {
    pub idempotency_key: String,
    pub recipe_instance_id: String,
    pub source_party_id: String,
    /// Target entity. Must equal this database's `entity.id`. Checked before the
    /// §3.9 hash. Not stored: the inbox target column is not in the schema yet.
    pub counterpart_entity_id: String,
    pub action: InboxAction,
    pub amount_minor: i64,
    pub currency: String,
    /// Required on the first apply. Ignored when the key is already stored.
    pub event_id: Option<String>,
}

struct FreezeRow {
    id: String,
    active: i64,
    code: String,
    commit_id: String,
    seq: i64,
}

struct Staged {
    new_rows: u32,
    inbox_new: u32,
    event_ids: Vec<String>,
    subjects: Vec<Subject>,
}

impl Staged {
    fn empty() -> Self {
        Self {
            new_rows: 0,
            inbox_new: 0,
            event_ids: Vec::new(),
            subjects: Vec::new(),
        }
    }
}

struct AccountSnap {
    account_type: String,
    financial_account_id: Option<String>,
}

/// One entity database. Reads stay available when writes are refused.
pub struct Kernel {
    conn: Connection,
    clock: Box<dyn Clock>,
    ids: Box<dyn IdSource>,
    actor: String,
    writable: bool,
    write_open: bool,
    freezing: bool,
}

/// Open write transaction. Dropping it rolls the attempt back.
pub struct Transaction<'a> {
    kernel: &'a mut Kernel,
    expected_revision: i64,
    recorded_at: String,
    commit_id: String,
    staged: Staged,
    finished: bool,
    error: Option<Error>,
}

impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Kernel")
    }
}

impl std::fmt::Debug for Transaction<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Transaction")
    }
}

impl Kernel {
    pub fn open(
        path: impl AsRef<Path>,
        clock: Box<dyn Clock>,
        ids: Box<dyn IdSource>,
        actor: &str,
    ) -> Result<Self, Error> {
        Self::open_inner(path, clock, ids, actor, true)
    }

    #[cfg(test)]
    pub(crate) fn open_observing_disabled_foreign_keys(
        path: impl AsRef<Path>,
        clock: Box<dyn Clock>,
        ids: Box<dyn IdSource>,
        actor: &str,
    ) -> Result<Self, Error> {
        Self::open_inner(path, clock, ids, actor, false)
    }

    fn open_inner(
        path: impl AsRef<Path>,
        clock: Box<dyn Clock>,
        ids: Box<dyn IdSource>,
        actor: &str,
        enable_foreign_keys: bool,
    ) -> Result<Self, Error> {
        if actor.is_empty() || actor.contains('\n') {
            return Err(Error::new(Code::InvalidShape, &[], vec![]));
        }
        let conn =
            Connection::open(path).map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
        conn.busy_timeout(Duration::from_millis(0))
            .map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
        ensure_schema(&conn)?;
        let pragma = if enable_foreign_keys {
            "PRAGMA foreign_keys = ON"
        } else {
            "PRAGMA foreign_keys = OFF"
        };
        conn.execute_batch(pragma)
            .map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
        let flag = read_fk(&conn)?;
        Ok(Self {
            conn,
            clock,
            ids,
            actor: actor.to_string(),
            writable: flag == 1,
            write_open: false,
            freezing: false,
        })
    }

    pub fn is_writable(&self) -> bool {
        self.writable
    }

    pub fn revision(&self) -> Result<i64, Error> {
        self.max_seq()
    }

    pub fn commit_count(&self) -> Result<i64, Error> {
        self.scalar("SELECT count(*) FROM commits")
    }

    pub fn commit_at_seq(&self, seq: i64) -> Result<Option<CommitInfo>, Error> {
        self.conn
            .query_row(
                "SELECT id, expected_revision, seq, kind, recorded_at FROM commits WHERE seq = ?1",
                [seq],
                |row| {
                    Ok(CommitInfo {
                        id: row.get(0)?,
                        expected_revision: row.get(1)?,
                        seq: row.get(2)?,
                        kind: row.get(3)?,
                        recorded_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn entity(&self) -> Result<Option<EntityRow>, Error> {
        self.conn
            .query_row(
                "SELECT id, party_id, name, created_at FROM entity",
                [],
                |row| {
                    Ok(EntityRow {
                        id: row.get(0)?,
                        party_id: row.get(1)?,
                        name: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn party(&self, id: &str) -> Result<Option<PartyRow>, Error> {
        self.conn
            .query_row(
                "SELECT id, name, label, parent_id, created_at FROM parties WHERE id = ?1",
                [id],
                |row| {
                    Ok(PartyRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        label: row.get(2)?,
                        parent_id: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn financial_account(&self, id: &str) -> Result<Option<FinancialAccountRow>, Error> {
        self.conn
            .query_row(
                "SELECT id, name, kind, currency, tracking_mode, last4 FROM financial_accounts WHERE id = ?1",
                [id],
                |row| {
                    Ok(FinancialAccountRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        kind: row.get(2)?,
                        currency: row.get(3)?,
                        tracking_mode: row.get(4)?,
                        last4: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn ledger_account(&self, id: &str) -> Result<Option<LedgerAccountRow>, Error> {
        self.conn
            .query_row(
                "SELECT id, financial_account_id, account_type, kind, parent_id, name
                 FROM ledger_accounts WHERE id = ?1",
                [id],
                |row| {
                    Ok(LedgerAccountRow {
                        id: row.get(0)?,
                        financial_account_id: row.get(1)?,
                        account_type: row.get(2)?,
                        kind: row.get(3)?,
                        parent_id: row.get(4)?,
                        name: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn current_name(&self, kind: ReferenceKind, id: &str) -> Result<Option<String>, Error> {
        let renamed: Option<String> = self
            .conn
            .query_row(
                "SELECT name FROM reference_current_names WHERE target_kind = ?1 AND target_id = ?2",
                params![kind.as_str(), id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))?;
        if renamed.is_some() {
            return Ok(renamed);
        }
        let sql = match kind {
            ReferenceKind::Entity => "SELECT name FROM entity WHERE id = ?1",
            ReferenceKind::Party => "SELECT name FROM parties WHERE id = ?1",
            ReferenceKind::FinancialAccount => "SELECT name FROM financial_accounts WHERE id = ?1",
            ReferenceKind::LedgerAccount => "SELECT name FROM ledger_accounts WHERE id = ?1",
            ReferenceKind::Tag => "SELECT name FROM tags WHERE id = ?1",
            ReferenceKind::Feed => "SELECT name FROM feeds WHERE id = ?1",
        };
        self.conn
            .query_row(sql, [id], |row| row.get(0))
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn posting_count(&self) -> Result<i64, Error> {
        self.scalar("SELECT count(*) FROM journal_postings")
    }

    pub fn inbox(&self, key: &str) -> Result<Option<InboxRow>, Error> {
        self.conn
            .query_row(
                "SELECT idempotency_key, recipe_instance_id, source_party_id, action, amount_minor,
                        currency, event_id, commit_id
                 FROM cross_entity_inbox WHERE idempotency_key = ?1",
                [key],
                |row| {
                    Ok(InboxRow {
                        idempotency_key: row.get(0)?,
                        recipe_instance_id: row.get(1)?,
                        source_party_id: row.get(2)?,
                        action: row.get(3)?,
                        amount_minor: row.get(4)?,
                        currency: row.get(5)?,
                        event_id: row.get(6)?,
                        commit_id: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn inbox_count(&self) -> Result<i64, Error> {
        self.scalar("SELECT count(*) FROM cross_entity_inbox")
    }

    pub fn freeze(&self) -> Result<Option<FreezeOutcome>, Error> {
        Ok(self.latest_freeze()?.map(|row| FreezeOutcome {
            row_id: row.id,
            commit_id: row.commit_id,
            seq: row.seq,
            code: row.code,
            already_present: row.active == 1,
        }))
    }

    pub fn kernel_state_count(&self) -> Result<i64, Error> {
        self.scalar("SELECT count(*) FROM kernel_state")
    }

    /// `None` is `MissingExpectedRevision`. The kernel does not read `max(seq)` to fill it in.
    pub fn begin_transaction(
        &mut self,
        expected_revision: Option<i64>,
    ) -> Result<Transaction<'_>, Error> {
        let expected = match expected_revision {
            Some(value) => value,
            None => return Err(Error::new(Code::MissingExpectedRevision, &[], vec![])),
        };
        self.prepare_write()?;
        if let Err(err) = self.gate_normal(expected) {
            self.rollback();
            return Err(err);
        }
        let recorded_at = self.clock.now_utc();
        if !self.timestamp_ok(&recorded_at)? {
            self.rollback();
            return Err(Error::new(Code::InvalidDate, &[], vec![]));
        }
        let commit_id = self.ids.next_id();
        if !is_ulid(&commit_id) {
            self.rollback();
            return Err(Error::new(Code::InvalidId, &[], vec![]));
        }
        self.write_open = true;
        Ok(Transaction {
            kernel: self,
            expected_revision: expected,
            recorded_at,
            commit_id,
            staged: Staged::empty(),
            finished: false,
            error: None,
        })
    }

    pub fn record_write_freeze(
        &mut self,
        expected_revision: Option<i64>,
        code: &str,
    ) -> Result<FreezeOutcome, Error> {
        let expected = match expected_revision {
            Some(value) => value,
            None => return Err(Error::new(Code::MissingExpectedRevision, &[], vec![])),
        };
        let parsed = Code::parse(code).filter(|c| c.class() == Class::Bugcheck);
        if parsed.is_none() {
            return Err(Error::new(Code::InvalidShape, &[], safe_token(code)));
        }
        if !self.writable {
            return Err(foreign_keys_disabled());
        }
        self.freezing = true;
        let result = self.record_freeze_locked(expected, code);
        self.freezing = false;
        result
    }

    pub fn clear_write_freeze(
        &mut self,
        expected_revision: Option<i64>,
    ) -> Result<CommitResult, Error> {
        let expected = match expected_revision {
            Some(value) => value,
            None => return Err(Error::new(Code::MissingExpectedRevision, &[], vec![])),
        };
        if !self.writable {
            return Err(foreign_keys_disabled());
        }
        if self.write_open {
            return Err(Error::new(Code::WriteLocked, &[], vec![]));
        }
        self.prepare_write()?;
        let active = match self.latest_freeze()? {
            Some(row) if row.active == 1 => row,
            _ => {
                self.rollback();
                return Err(Error::new(Code::InvalidShape, &[], vec![]));
            }
        };
        // Compare only. Do not assign max(seq) to the caller's revision.
        let current = self.max_seq()?;
        if current != expected {
            self.rollback();
            return Err(stale_revision());
        }
        if let Err(err) = self.blocking_bugcheck() {
            self.rollback();
            return Err(err);
        }
        let recorded_at = self.clock.now_utc();
        if !self.timestamp_ok(&recorded_at)? {
            self.rollback();
            return Err(Error::new(Code::InvalidDate, &[], vec![]));
        }
        let commit_id = self.ids.next_id();
        let row_id = self.ids.next_id();
        if !is_ulid(&commit_id) || !is_ulid(&row_id) {
            self.rollback();
            return Err(Error::new(Code::InvalidId, &[], vec![]));
        }
        if let Err(err) = self.insert_freeze(&row_id, 0, &active.code, &commit_id, &recorded_at) {
            self.rollback();
            return Err(err);
        }
        if let Err(err) = self.insert_commit(&commit_id, expected, "freeze", &recorded_at) {
            self.rollback();
            return Err(self.after_seal_error(expected, err));
        }
        self.commit_sql()?;
        Ok(CommitResult {
            noop: false,
            commit_id: Some(commit_id),
            seq: expected + 1,
            subjects: Vec::new(),
        })
    }

    fn record_freeze_locked(&mut self, expected: i64, code: &str) -> Result<FreezeOutcome, Error> {
        if self.write_open {
            return Err(Error::new(Code::WriteLocked, &[], vec![]));
        }
        self.prepare_write()?;
        if let Some(row) = self.latest_freeze()? {
            if row.active == 1 {
                self.rollback();
                return Ok(FreezeOutcome {
                    row_id: row.id,
                    commit_id: row.commit_id,
                    seq: row.seq,
                    code: row.code,
                    already_present: true,
                });
            }
        }
        let current = self.max_seq()?;
        if current != expected {
            self.rollback();
            return Err(stale_revision());
        }
        let recorded_at = self.clock.now_utc();
        if !self.timestamp_ok(&recorded_at)? {
            self.rollback();
            return Err(Error::new(Code::InvalidDate, &[], vec![]));
        }
        let commit_id = self.ids.next_id();
        let row_id = self.ids.next_id();
        if !is_ulid(&commit_id) || !is_ulid(&row_id) {
            self.rollback();
            return Err(Error::new(Code::InvalidId, &[], vec![]));
        }
        if let Err(err) = self.insert_freeze(&row_id, 1, code, &commit_id, &recorded_at) {
            self.rollback();
            return Err(err);
        }
        if let Err(err) = self.insert_commit(&commit_id, expected, "freeze", &recorded_at) {
            self.rollback();
            return Err(err);
        }
        self.commit_sql()?;
        Ok(FreezeOutcome {
            row_id,
            commit_id,
            seq: expected + 1,
            code: code.to_string(),
            already_present: false,
        })
    }

    fn prepare_write(&mut self) -> Result<(), Error> {
        if !self.writable {
            return Err(foreign_keys_disabled());
        }
        if self.write_open {
            return Err(Error::new(Code::WriteLocked, &[], vec![]));
        }
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| map_sql(e, SqlSite::Begin))?;
        match read_fk(&self.conn) {
            Ok(1) => Ok(()),
            Ok(_) => {
                self.rollback();
                Err(foreign_keys_disabled())
            }
            Err(err) => {
                self.rollback();
                Err(err)
            }
        }
    }

    fn gate_normal(&mut self, expected: i64) -> Result<(), Error> {
        if let Some(row) = self.latest_freeze()? {
            if row.active == 1 {
                return Err(write_frozen(&row));
            }
        }
        // Compare only. Never assign this read to expected_revision.
        let current = self.max_seq()?;
        if current != expected {
            return Err(stale_revision());
        }
        Ok(())
    }

    fn blocking_bugcheck(&self) -> Result<(), Error> {
        // U21 owns doctor, including evidence bytes and cross-entity pairing.
        // Clear refuses when the checks this database can answer are dirty.
        let integrity: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|e| map_sql(e, SqlSite::Read))?;
        if integrity != "ok" {
            return Err(Error::new(Code::IntegrityFailed, &[], vec![]));
        }
        let fk: i64 = self
            .conn
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .map_err(|e| map_sql(e, SqlSite::Read))?;
        if fk > 0 {
            return Err(Error::new(Code::ForeignKeyCheckFailed, &["PR-19"], vec![]));
        }
        let views: &[(&str, Code, &[&str])] = &[
            ("doctor_unsealed_rows", Code::UnsealedRows, &["PR-19"]),
            ("doctor_unbalanced_events", Code::DoctorDrift, &["INV-001"]),
            ("doctor_superseded_drift", Code::DoctorDrift, &["ADR 0002"]),
            (
                "doctor_observation_double_consumed",
                Code::DoctorDrift,
                &["INV-007"],
            ),
        ];
        for (view, code, refs) in views {
            let sql = format!("SELECT count(*) FROM {view}");
            let n: i64 = self
                .conn
                .query_row(&sql, [], |row| row.get(0))
                .map_err(|e| map_sql(e, SqlSite::Read))?;
            if n > 0 {
                return Err(Error::new(*code, refs, vec![]));
            }
        }
        Ok(())
    }

    fn maybe_persist_freeze(&mut self, expected: i64, err: &Error) {
        if self.freezing || !persists_freeze(err.code) {
            return;
        }
        let _ = self.record_write_freeze(Some(expected), err.code.as_str());
    }

    fn after_seal_error(&mut self, expected: i64, err: Error) -> Error {
        if persists_freeze(err.code) {
            self.maybe_persist_freeze(expected, &err);
        }
        err
    }

    fn insert_freeze(
        &self,
        row_id: &str,
        active: i64,
        code: &str,
        commit_id: &str,
        recorded_at: &str,
    ) -> Result<(), Error> {
        self.conn
            .execute(
                "INSERT INTO kernel_state(id, state_key, active, code, commit_id, recorded_at)
                 VALUES (?1, 'freeze', ?2, ?3, ?4, ?5)",
                params![row_id, active, code, commit_id, recorded_at],
            )
            .map(|_| ())
            .map_err(|e| map_sql(e, SqlSite::Staging))
    }

    fn insert_commit(
        &self,
        commit_id: &str,
        expected: i64,
        kind: &str,
        recorded_at: &str,
    ) -> Result<(), Error> {
        self.conn
            .execute(
                "INSERT INTO commits(id, expected_revision, seq, kind, actor, reason, recorded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, '', ?6)",
                params![
                    commit_id,
                    expected,
                    expected + 1,
                    kind,
                    self.actor,
                    recorded_at
                ],
            )
            .map(|_| ())
            .map_err(|e| map_sql(e, SqlSite::Seal))
    }

    fn commit_sql(&self) -> Result<(), Error> {
        self.conn.execute_batch("COMMIT").map_err(|e| {
            map_sql(
                e,
                SqlSite::Finalize {
                    commits_inserted: true,
                },
            )
        })
    }

    fn rollback(&self) {
        let _ = self.conn.execute_batch("ROLLBACK");
    }

    fn max_seq(&self) -> Result<i64, Error> {
        self.scalar("SELECT coalesce(max(seq), 0) FROM commits")
    }

    fn scalar(&self, sql: &str) -> Result<i64, Error> {
        self.conn
            .query_row(sql, [], |row| row.get(0))
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    fn latest_freeze(&self) -> Result<Option<FreezeRow>, Error> {
        self.conn
            .query_row(
                "SELECT k.id, k.active, k.code, k.commit_id, c.seq
                 FROM kernel_state k
                 JOIN commits c ON c.id = k.commit_id
                 WHERE k.state_key = 'freeze'
                 ORDER BY c.seq DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok(FreezeRow {
                        id: row.get(0)?,
                        active: row.get(1)?,
                        code: row.get(2)?,
                        commit_id: row.get(3)?,
                        seq: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    fn timestamp_ok(&self, value: &str) -> Result<bool, Error> {
        self.conn
            .query_row(
                "SELECT (?1 GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'
                        AND julianday(?1) IS NOT NULL)",
                [value],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n == 1)
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    fn date_ok(&self, value: &str) -> Result<bool, Error> {
        self.conn
            .query_row("SELECT date(?1) IS ?1", [value], |row| row.get::<_, i64>(0))
            .map(|n| n == 1)
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }
}

impl Transaction<'_> {
    pub fn create_entity(&mut self, new: NewEntity) -> Result<String, Error> {
        self.ensure_open()?;
        require_name(&new.name).map_err(|e| self.fail(e))?;
        let id = self.allocate(new.id)?;
        let party_id = self.allocate(new.party_id)?;
        if id == party_id {
            return Err(self.fail(shape("unique_other")));
        }
        if self.count("SELECT count(*) FROM entity", &[])? > 0 {
            return Err(self.fail(Error::new(Code::SecondEntity, &["ADR 0004"], vec![id])));
        }
        let recorded_at = self.recorded_at.clone();
        self.exec(
            "INSERT INTO entity(id, party_id, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, party_id, new.name, recorded_at],
        )?;
        self.staged.new_rows += 1;
        Ok(id)
    }

    pub fn create_party(&mut self, new: NewParty) -> Result<String, Error> {
        self.ensure_open()?;
        require_name(&new.name).map_err(|e| self.fail(e))?;
        let id = self.allocate(new.id)?;
        if let Some(parent) = &new.parent_id {
            require_ulid(parent).map_err(|e| self.fail(e))?;
            if parent == &id
                || self.count(
                    "SELECT count(*) FROM parties WHERE id = ?1",
                    &[parent.as_str()],
                )? == 0
            {
                return Err(self.fail(Error::new(Code::UnknownTarget, &[], vec![parent.clone()])));
            }
        }
        if self.count("SELECT count(*) FROM parties WHERE id = ?1", &[id.as_str()])? > 0 {
            return Err(self.fail(Error::new(Code::DuplicateId, &[], vec![id])));
        }
        let recorded_at = self.recorded_at.clone();
        self.exec(
            "INSERT INTO parties(id, name, label, parent_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, new.name, new.label.as_str(), new.parent_id, recorded_at],
        )?;
        self.staged.new_rows += 1;
        Ok(id)
    }

    pub fn add_party_alias(&mut self, party_id: &str, alias: &str) -> Result<(), Error> {
        self.ensure_open()?;
        require_ulid(party_id).map_err(|e| self.fail(e))?;
        require_name(alias).map_err(|e| self.fail(e))?;
        if self.count("SELECT count(*) FROM parties WHERE id = ?1", &[party_id])? == 0 {
            return Err(self.fail(Error::new(
                Code::UnknownTarget,
                &[],
                vec![party_id.to_string()],
            )));
        }
        if self.count(
            "SELECT count(*) FROM party_aliases WHERE party_id = ?1 AND alias = ?2",
            &[party_id, alias],
        )? > 0
        {
            return Err(self.fail(shape("unique_other")));
        }
        self.exec(
            "INSERT INTO party_aliases(party_id, alias) VALUES (?1, ?2)",
            params![party_id, alias],
        )?;
        self.staged.new_rows += 1;
        Ok(())
    }

    pub fn rename(
        &mut self,
        kind: ReferenceKind,
        target_id: &str,
        name: &str,
    ) -> Result<String, Error> {
        self.ensure_open()?;
        require_ulid(target_id).map_err(|e| self.fail(e))?;
        require_name(name).map_err(|e| self.fail(e))?;
        if !self.target_exists(kind, target_id)? {
            return Err(self.fail(Error::new(
                Code::RenameTargetMissing,
                &[],
                vec![target_id.to_string()],
            )));
        }
        let id = self.allocate(None)?;
        let recorded_at = self.recorded_at.clone();
        self.exec(
            "INSERT INTO reference_renames(id, target_kind, target_id, name, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, kind.as_str(), target_id, name, recorded_at],
        )?;
        self.staged.new_rows += 1;
        Ok(id)
    }

    pub fn create_financial_account(&mut self, new: NewFinancialAccount) -> Result<String, Error> {
        self.ensure_open()?;
        require_name(&new.name).map_err(|e| self.fail(e))?;
        require_currency(&new.currency).map_err(|e| self.fail(e))?;
        match &new.last4 {
            None => {}
            Some(last4) if last4.len() == 4 && last4.bytes().all(|b| b.is_ascii_digit()) => {}
            Some(_) => return Err(self.fail(shape("last4"))),
        }
        let institution = new.institution.filter(|value| !value.is_empty());
        let id = self.allocate(new.id)?;
        if self.count(
            "SELECT count(*) FROM financial_accounts WHERE id = ?1",
            &[id.as_str()],
        )? > 0
        {
            return Err(self.fail(Error::new(Code::DuplicateId, &[], vec![id])));
        }
        let recorded_at = self.recorded_at.clone();
        self.exec(
            "INSERT INTO financial_accounts(id, name, kind, currency, tracking_mode, institution, last4, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                new.name,
                new.kind.as_str(),
                new.currency,
                new.tracking_mode.as_str(),
                institution,
                new.last4,
                recorded_at
            ],
        )?;
        self.staged.new_rows += 1;
        Ok(id)
    }

    pub fn create_ledger_account(&mut self, new: NewLedgerAccount) -> Result<String, Error> {
        self.ensure_open()?;
        require_name(&new.name).map_err(|e| self.fail(e))?;
        if !new.kind.fits(new.account_type) {
            return Err(self.fail(shape("ledger_kind")));
        }
        let id = self.allocate(new.id)?;
        if let Some(parent) = &new.parent_id {
            if new.kind != LedgerKind::Category {
                return Err(self.fail(shape("ledger_kind")));
            }
            require_ulid(parent).map_err(|e| self.fail(e))?;
            if parent == &id {
                return Err(self.fail(Error::new(
                    Code::CategoryParentInvalid,
                    &["INV-003"],
                    vec![parent.clone()],
                )));
            }
            let Some(snap) = self.account_snap(parent)? else {
                return Err(self.fail(Error::new(
                    Code::CategoryParentInvalid,
                    &["INV-003"],
                    vec![parent.clone()],
                )));
            };
            if snap.account_type != new.account_type.as_str() {
                return Err(self.fail(Error::new(
                    Code::CategoryParentInvalid,
                    &["INV-003", "ADR 0008"],
                    vec![parent.clone()],
                )));
            }
            if self.count(
                "SELECT count(*) FROM journal_postings WHERE ledger_account_id = ?1",
                &[parent.as_str()],
            )? > 0
            {
                return Err(self.fail(Error::new(
                    Code::CategoryParentInvalid,
                    &["INV-003"],
                    vec![parent.clone()],
                )));
            }
        }
        if self.singleton_entity()?.is_none() {
            return Err(self.fail(Error::new(
                Code::DatabaseUninitialized,
                &["ADR 0004"],
                vec![],
            )));
        }
        if let Some(financial_id) = &new.financial_account_id {
            if !new.kind.allows_financial_account() {
                return Err(self.fail(shape("ledger_kind")));
            }
            require_ulid(financial_id).map_err(|e| self.fail(e))?;
            let Some((currency, fin_kind)) = self.financial_kind(financial_id)? else {
                return Err(self.fail(Error::new(
                    Code::UnknownTarget,
                    &[],
                    vec![financial_id.clone()],
                )));
            };
            let Ok(parsed) = FinancialKind::parse(&fin_kind) else {
                return Err(self.fail(Error::new(Code::UnmappedStorageError, &[], vec![])));
            };
            if !ledger_matches_financial(new.kind, parsed) {
                return Err(self.fail(Error::new(
                    Code::LedgerKindMismatch,
                    &["ADR 0008"],
                    vec![financial_id.clone()],
                )));
            }
            let _ = currency;
        }
        if self.count(
            "SELECT count(*) FROM ledger_accounts WHERE id = ?1",
            &[id.as_str()],
        )? > 0
        {
            return Err(self.fail(Error::new(Code::DuplicateId, &[], vec![id])));
        }
        let recorded_at = self.recorded_at.clone();
        self.exec(
            "INSERT INTO ledger_accounts(
                id, financial_account_id, account_type, kind, parent_id, name, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                new.financial_account_id,
                new.account_type.as_str(),
                new.kind.as_str(),
                new.parent_id,
                new.name,
                recorded_at
            ],
        )?;
        self.staged.new_rows += 1;
        Ok(id)
    }

    pub fn register_recipe(&mut self, id: &str, version: &str) -> Result<(), Error> {
        self.ensure_open()?;
        if id.is_empty() || version.is_empty() || id.contains('\n') || version.contains('\n') {
            return Err(self.fail(Error::new(Code::InvalidShape, &[], vec![])));
        }
        if self.count(
            "SELECT count(*) FROM recipes WHERE id = ?1 AND version = ?2",
            &[id, version],
        )? > 0
        {
            return Err(self.fail(Error::new(Code::DuplicateId, &[], vec![id.to_string()])));
        }
        self.exec(
            "INSERT INTO recipes(id, version) VALUES (?1, ?2)",
            params![id, version],
        )?;
        self.staged.new_rows += 1;
        Ok(())
    }

    /// Minimum event insert for INV-003 and the inbox stub. Not `append_event`.
    pub fn stage_event(&mut self, new: NewEvent) -> Result<String, Error> {
        self.ensure_open()?;
        if new.recipe_id.is_empty() || new.recipe_version.is_empty() {
            return Err(self.fail(Error::new(Code::UnknownRecipe, &[], vec![])));
        }
        if self.count(
            "SELECT count(*) FROM recipes WHERE id = ?1 AND version = ?2",
            &[new.recipe_id.as_str(), new.recipe_version.as_str()],
        )? == 0
        {
            return Err(self.fail(Error::new(
                Code::UnknownRecipe,
                &[],
                vec![new.recipe_id.clone()],
            )));
        }
        if !self
            .kernel
            .date_ok(&new.effective_date)
            .map_err(|e| self.fail(e))?
        {
            return Err(self.fail(Error::new(Code::InvalidDate, &[], vec![])));
        }
        if new.postings.len() < 2 {
            return Err(self.fail(Error::new(Code::InsufficientPostings, &["INV-002"], vec![])));
        }
        let mut prepared = Vec::with_capacity(new.postings.len());
        let mut sums: BTreeMap<String, i64> = BTreeMap::new();
        for posting in &new.postings {
            require_ulid(&posting.ledger_account_id).map_err(|e| self.fail(e))?;
            require_currency(&posting.currency).map_err(|e| self.fail(e))?;
            require_positive(posting.amount_minor).map_err(|e| self.fail(e))?;
            let Some(account) = self.account_snap(&posting.ledger_account_id)? else {
                return Err(self.fail(Error::new(
                    Code::UnknownTarget,
                    &[],
                    vec![posting.ledger_account_id.clone()],
                )));
            };
            if matches!(account.account_type.as_str(), "income" | "expense")
                && self.count(
                    "SELECT count(*) FROM ledger_accounts WHERE parent_id = ?1",
                    &[posting.ledger_account_id.as_str()],
                )? > 0
            {
                return Err(self.fail(Error::new(
                    Code::NonLeafCategory,
                    &["INV-003"],
                    vec![posting.ledger_account_id.clone()],
                )));
            }
            if let Some(financial_id) = &account.financial_account_id {
                let Some((currency, _)) = self.financial_kind(financial_id)? else {
                    return Err(self.fail(Error::new(
                        Code::UnknownTarget,
                        &[],
                        vec![financial_id.clone()],
                    )));
                };
                if currency != posting.currency {
                    return Err(self.fail(Error::new(
                        Code::CurrencyMismatch,
                        &["INV-005"],
                        vec![posting.ledger_account_id.clone(), financial_id.clone()],
                    )));
                }
            }
            if let Some(party_id) = &posting.party_id {
                require_ulid(party_id).map_err(|e| self.fail(e))?;
                if self.count(
                    "SELECT count(*) FROM parties WHERE id = ?1",
                    &[party_id.as_str()],
                )? == 0
                {
                    return Err(self.fail(Error::new(
                        Code::UnknownTarget,
                        &[],
                        vec![party_id.clone()],
                    )));
                }
            }
            let signed = if posting.side == Side::Debit {
                posting.amount_minor
            } else {
                -posting.amount_minor
            };
            let entry = sums.entry(posting.currency.clone()).or_insert(0);
            *entry = entry
                .checked_add(signed)
                .ok_or_else(|| self.fail(Error::new(Code::InvalidMoney, &[], vec![])))?;
            prepared.push(posting);
        }
        if sums.values().any(|net| *net != 0) {
            return Err(self.fail(Error::new(Code::Unbalanced, &["INV-001"], vec![])));
        }
        let event_id = self.allocate(new.id)?;
        if self.count(
            "SELECT count(*) FROM accounting_events WHERE id = ?1",
            &[event_id.as_str()],
        )? > 0
        {
            return Err(self.fail(Error::new(Code::DuplicateId, &[], vec![event_id])));
        }
        let recorded_at = self.recorded_at.clone();
        let commit_id = self.commit_id.clone();
        self.exec(
            "INSERT INTO accounting_events(
                id, commit_id, recipe_id, recipe_version, effective_date, description, recorded_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, '', ?6)",
            params![
                event_id,
                commit_id,
                new.recipe_id,
                new.recipe_version,
                new.effective_date,
                recorded_at
            ],
        )?;
        self.staged.new_rows += 1;
        for posting in prepared {
            let posting_id = self.allocate(posting.id.clone())?;
            self.exec(
                "INSERT INTO journal_postings(
                    id, event_id, ledger_account_id, currency, side, amount_minor, party_id, memo
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '')",
                params![
                    posting_id,
                    event_id,
                    posting.ledger_account_id,
                    posting.currency,
                    posting.side.as_str(),
                    posting.amount_minor,
                    posting.party_id
                ],
            )?;
            self.staged.new_rows += 1;
        }
        self.staged.event_ids.push(event_id.clone());
        Ok(event_id)
    }

    /// Inbox apply. `counterpart_entity_id` must equal `entity.id` before the
    /// §3.9 hash compare. The caller stages the counterpart event with `stage_event`.
    pub fn apply_inbox(&mut self, apply: InboxApply) -> Result<(), Error> {
        self.ensure_open()?;
        if apply.idempotency_key.is_empty() || apply.idempotency_key.contains('\n') {
            return Err(self.fail(Error::new(Code::InvalidShape, &[], vec![])));
        }
        require_ulid(&apply.recipe_instance_id).map_err(|e| self.fail(e))?;
        require_ulid(&apply.source_party_id).map_err(|e| self.fail(e))?;
        require_ulid(&apply.counterpart_entity_id).map_err(|e| self.fail(e))?;
        require_positive(apply.amount_minor).map_err(|e| self.fail(e))?;
        require_currency(&apply.currency).map_err(|e| self.fail(e))?;
        let entity_id = self.singleton_entity()?;
        if entity_id.as_deref() != Some(apply.counterpart_entity_id.as_str()) {
            return Err(self.fail(Error::new(
                Code::EntityMismatch,
                &["ADR 0004"],
                vec![apply.counterpart_entity_id],
            )));
        }
        let incoming = inbox_payload_hash(
            &apply.recipe_instance_id,
            &apply.source_party_id,
            apply.action.as_str(),
            apply.amount_minor,
            &apply.currency,
        )
        .map_err(|e| self.fail(e))?;
        if let Some(stored) = self.inbox_row(&apply.idempotency_key)? {
            let stored_hash = inbox_payload_hash(
                &stored.recipe_instance_id,
                &stored.source_party_id,
                &stored.action,
                stored.amount_minor,
                &stored.currency,
            )
            .map_err(|e| self.fail(e))?;
            if stored_hash != incoming {
                return Err(self.fail(Error::new(
                    Code::IdempotencyConflict,
                    &["ADR 0004"],
                    vec![apply.idempotency_key, apply.recipe_instance_id],
                )));
            }
            let seq = self.seq_of(&stored.commit_id)?;
            self.staged.subjects.push(Subject::IdempotencyKey {
                idempotency_key: apply.idempotency_key,
                commit_id: stored.commit_id,
                seq,
            });
            return Ok(());
        }
        let Some(event_id) = apply.event_id else {
            return Err(self.fail(Error::new(Code::UnknownTarget, &[], vec![])));
        };
        require_ulid(&event_id).map_err(|e| self.fail(e))?;
        if !self.staged.event_ids.iter().any(|id| id == &event_id) {
            return Err(self.fail(Error::new(Code::UnknownTarget, &[], vec![event_id])));
        }
        if self.count(
            "SELECT count(*) FROM parties WHERE id = ?1",
            &[apply.source_party_id.as_str()],
        )? == 0
        {
            return Err(self.fail(Error::new(
                Code::UnknownTarget,
                &[],
                vec![apply.source_party_id],
            )));
        }
        let recorded_at = self.recorded_at.clone();
        let commit_id = self.commit_id.clone();
        self.exec(
            "INSERT INTO cross_entity_inbox(
                idempotency_key, recipe_instance_id, source_party_id, action, amount_minor,
                currency, event_id, commit_id, recorded_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                apply.idempotency_key,
                apply.recipe_instance_id,
                apply.source_party_id,
                apply.action.as_str(),
                apply.amount_minor,
                apply.currency,
                event_id,
                commit_id,
                recorded_at
            ],
        )?;
        self.staged.new_rows += 1;
        self.staged.inbox_new += 1;
        Ok(())
    }

    pub fn commit(mut self) -> Result<CommitResult, Error> {
        if let Some(err) = self.error.clone() {
            self.finished = true;
            return Err(err);
        }
        let result = self.commit_inner();
        self.finished = true;
        result
    }

    fn commit_inner(&mut self) -> Result<CommitResult, Error> {
        match read_fk(&self.kernel.conn) {
            Ok(1) => {}
            Ok(_) => return Err(self.fail(foreign_keys_disabled())),
            Err(err) => return Err(self.fail(err)),
        }
        if self.staged.new_rows == 0 {
            self.kernel.rollback();
            self.kernel.write_open = false;
            return Ok(CommitResult {
                noop: true,
                commit_id: None,
                seq: self.expected_revision,
                subjects: std::mem::take(&mut self.staged.subjects),
            });
        }
        let kind = if self.staged.inbox_new > 0 {
            "cross_entity"
        } else if !self.staged.event_ids.is_empty() {
            "interpret"
        } else {
            "setup"
        };
        if let Err(err) = self.kernel.insert_commit(
            &self.commit_id,
            self.expected_revision,
            kind,
            &self.recorded_at,
        ) {
            return Err(self.fail(err));
        }
        if let Err(err) = self.kernel.commit_sql() {
            return Err(self.fail(err));
        }
        self.kernel.write_open = false;
        Ok(CommitResult {
            noop: false,
            commit_id: Some(self.commit_id.clone()),
            seq: self.expected_revision + 1,
            subjects: Vec::new(),
        })
    }

    fn ensure_open(&self) -> Result<(), Error> {
        if let Some(err) = &self.error {
            return Err(err.clone());
        }
        Ok(())
    }

    fn fail(&mut self, err: Error) -> Error {
        let expected = self.expected_revision;
        self.kernel.rollback();
        self.kernel.write_open = false;
        self.finished = true;
        self.error = Some(err.clone());
        self.kernel.maybe_persist_freeze(expected, &err);
        err
    }

    fn allocate(&mut self, supplied: Option<String>) -> Result<String, Error> {
        let id = match supplied {
            Some(id) => id,
            None => self.kernel.ids.next_id(),
        };
        if is_ulid(&id) {
            Ok(id)
        } else {
            Err(self.fail(Error::new(Code::InvalidId, &[], vec![])))
        }
    }

    fn exec<P: rusqlite::Params>(&mut self, sql: &str, params: P) -> Result<(), Error> {
        match self.kernel.conn.execute(sql, params) {
            Ok(_) => Ok(()),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Staging))),
        }
    }

    fn count(&mut self, sql: &str, args: &[&str]) -> Result<i64, Error> {
        let result = match args {
            [] => self.kernel.conn.query_row(sql, [], |row| row.get(0)),
            [a] => self.kernel.conn.query_row(sql, [*a], |row| row.get(0)),
            [a, b] => self
                .kernel
                .conn
                .query_row(sql, params![*a, *b], |row| row.get(0)),
            _ => return Err(self.fail(Error::new(Code::StorageFault, &[], vec![]))),
        };
        match result {
            Ok(n) => Ok(n),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn singleton_entity(&mut self) -> Result<Option<String>, Error> {
        match self
            .kernel
            .conn
            .query_row("SELECT id FROM entity", [], |row| row.get(0))
            .optional()
        {
            Ok(value) => Ok(value),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn account_snap(&mut self, id: &str) -> Result<Option<AccountSnap>, Error> {
        match self
            .kernel
            .conn
            .query_row(
                "SELECT account_type, financial_account_id FROM ledger_accounts WHERE id = ?1",
                [id],
                |row| {
                    Ok(AccountSnap {
                        account_type: row.get(0)?,
                        financial_account_id: row.get(1)?,
                    })
                },
            )
            .optional()
        {
            Ok(value) => Ok(value),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn financial_kind(&mut self, id: &str) -> Result<Option<(String, String)>, Error> {
        match self
            .kernel
            .conn
            .query_row(
                "SELECT currency, kind FROM financial_accounts WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
        {
            Ok(value) => Ok(value),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn target_exists(&mut self, kind: ReferenceKind, id: &str) -> Result<bool, Error> {
        let sql = match kind {
            ReferenceKind::Entity => "SELECT count(*) FROM entity WHERE id = ?1",
            ReferenceKind::Party => "SELECT count(*) FROM parties WHERE id = ?1",
            ReferenceKind::FinancialAccount => {
                "SELECT count(*) FROM financial_accounts WHERE id = ?1"
            }
            ReferenceKind::LedgerAccount => "SELECT count(*) FROM ledger_accounts WHERE id = ?1",
            ReferenceKind::Tag => "SELECT count(*) FROM tags WHERE id = ?1",
            ReferenceKind::Feed => "SELECT count(*) FROM feeds WHERE id = ?1",
        };
        Ok(self.count(sql, &[id])? > 0)
    }

    fn inbox_row(&mut self, key: &str) -> Result<Option<InboxRow>, Error> {
        match self
            .kernel
            .conn
            .query_row(
                "SELECT idempotency_key, recipe_instance_id, source_party_id, action, amount_minor,
                        currency, event_id, commit_id
                 FROM cross_entity_inbox WHERE idempotency_key = ?1",
                [key],
                |row| {
                    Ok(InboxRow {
                        idempotency_key: row.get(0)?,
                        recipe_instance_id: row.get(1)?,
                        source_party_id: row.get(2)?,
                        action: row.get(3)?,
                        amount_minor: row.get(4)?,
                        currency: row.get(5)?,
                        event_id: row.get(6)?,
                        commit_id: row.get(7)?,
                    })
                },
            )
            .optional()
        {
            Ok(value) => Ok(value),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn seq_of(&mut self, commit_id: &str) -> Result<i64, Error> {
        match self.kernel.conn.query_row(
            "SELECT seq FROM commits WHERE id = ?1",
            [commit_id],
            |row| row.get(0),
        ) {
            Ok(seq) => Ok(seq),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.kernel.rollback();
            self.kernel.write_open = false;
        }
    }
}

pub fn inbox_payload_hash(
    recipe_instance_id: &str,
    source_party_id: &str,
    action: &str,
    amount_minor: i64,
    currency: &str,
) -> Result<String, Error> {
    let amount = crate::v2::types::canonical_amount(amount_minor)?;
    let fields = [
        recipe_instance_id,
        source_party_id,
        action,
        amount.as_str(),
        currency,
    ];
    if fields.iter().any(|field| field.contains('\n')) {
        return Err(Error::new(Code::InvalidShape, &[], vec![]));
    }
    let mut bytes = String::new();
    for field in fields {
        bytes.push_str(field);
        bytes.push('\n');
    }
    Ok(hex_encode(&Sha256::digest(bytes.as_bytes())))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

fn ensure_schema(conn: &Connection) -> Result<(), Error> {
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
    if tables == 0 {
        return conn
            .execute_batch(SCHEMA_SQL)
            .map_err(|_| Error::new(Code::StorageFault, &[], vec![]));
    }
    let freeze: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'kernel_state'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
    if freeze == 1 {
        Ok(())
    } else {
        Err(Error::new(Code::StorageFault, &[], vec![]))
    }
}

fn read_fk(conn: &Connection) -> Result<i64, Error> {
    conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|_| Error::new(Code::StorageFault, &[], vec![]))
}

fn foreign_keys_disabled() -> Error {
    Error::new(Code::ForeignKeysDisabled, &["PR-19"], vec![])
}

fn stale_revision() -> Error {
    Error::new(Code::StaleRevision, &["AT-032"], vec![])
}

fn write_frozen(row: &FreezeRow) -> Error {
    Error::new(
        Code::WriteFrozen,
        &[row.code.as_str()],
        vec![row.id.clone(), row.commit_id.clone()],
    )
}

fn safe_token(value: &str) -> Vec<String> {
    let ok =
        !value.is_empty() && value.len() <= 64 && value.bytes().all(|b| b.is_ascii_alphanumeric());
    if ok {
        vec![value.to_string()]
    } else {
        Vec::new()
    }
}
