//! SQLite adapter for the v2 write protocol, reference data, and append-only events.
//!
//! Reference rows are not commit-scoped (SPEC-v2-delta §5 Q5). Each successful
//! write still seals a `commits` row so the caller-supplied revision is checked
//! under `BEGIN IMMEDIATE` (ADR 0007). An identical recipe retry that stages
//! nothing else does not seal a commit (spec §2.2, §3.9).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};

use crate::v2::doctor::{exit_status, DoctorCheck, DoctorRunner, Finding, Scope, Severity};
use crate::v2::error::{persists_freeze, Class, Code, Error};
use crate::v2::participant::{
    admit_participant_error, in_context, install_authorizer, participants_ok, CommitStagedSet,
    ContextCall, ContextParticipant, ContextTx, KernelStaged, ParticipantPayloads, StageOp,
    StagedContextRow, StagedKernelEvent,
};
use crate::v2::sqlmap::{map_sql, SqlSite};
use crate::v2::types::{
    is_ulid, ledger_matches_financial, require_currency, require_name, require_positive,
    require_ulid, shape, AccountType, Clock, FinancialKind, IdSource, LedgerKind, PartyLabel,
    ReferenceKind, Side, TrackingMode,
};

const SCHEMA_SQL: &str = include_str!("../../../../../docs/schema-v2.sql");

pub use crate::v2::participant::Subject;

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
    /// Event fact. Compared on a recipe retry. Empty is allowed.
    pub description: String,
    pub postings: Vec<NewPosting>,
    /// Shared ULID of a cross-entity recipe half. Set together with `counterpart_entity_id`, or both absent.
    pub recipe_instance_id: Option<String>,
    pub counterpart_entity_id: Option<String>,
    /// One row per currency. Required when the two recipe fields are set, and empty otherwise.
    pub expectations: Vec<CounterpartExpectation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterpartExpectation {
    pub currency: String,
    pub expected_amount_minor: i64,
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
    context_rows: u32,
    event_ids: Vec<String>,
    subjects: Vec<Subject>,
    cross_entity: bool,
    correcting: bool,
    kernel_events: Vec<StagedKernelEvent>,
    superseded_targets: Vec<String>,
    context_staged: BTreeMap<String, Vec<StagedContextRow>>,
}

impl Staged {
    fn empty() -> Self {
        Self {
            new_rows: 0,
            context_rows: 0,
            event_ids: Vec::new(),
            subjects: Vec::new(),
            cross_entity: false,
            correcting: false,
            kernel_events: Vec::new(),
            superseded_targets: Vec::new(),
            context_staged: BTreeMap::new(),
        }
    }
}

struct WrittenEvent {
    id: String,
    line_ids: Vec<String>,
}

struct AccountSnap {
    account_type: String,
    kind: String,
    financial_account_id: Option<String>,
}

struct RecipeHalf {
    recipe_instance_id: String,
    counterpart_entity_id: String,
    expectations: Vec<(String, i64)>,
}

struct StoredHalf {
    id: String,
    commit_id: String,
    recipe_id: String,
    recipe_version: String,
    effective_date: String,
    description: String,
    recipe_instance_id: String,
    counterpart_entity_id: String,
}

struct TargetEvent {
    id: String,
    recipe_id: String,
    recipe_version: String,
    effective_date: String,
    recipe_instance_id: Option<String>,
    counterpart_entity_id: Option<String>,
    reverses_id: Option<String>,
    superseded_by: Option<String>,
}

struct StoredPosting {
    ledger_account_id: String,
    currency: String,
    side: String,
    amount_minor: i64,
    party_id: Option<String>,
    memo: String,
}

/// Doctor result. `exit_status` is 0 only when every finding is a warning or
/// info, or there are no findings. Checks do not repair rows or clear a freeze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub findings: Vec<Finding>,
    /// 0 when clean (warnings and info allowed). 1 when a bugcheck or import finding is present.
    pub exit_status: i32,
    /// Set when this call sealed a new freeze commit.
    pub freeze: Option<FreezeOutcome>,
    /// Set when a new bugcheck should have frozen the database and the persist did not.
    /// The findings are still returned.
    pub persist_error: Option<Error>,
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
    doctor_checks: Vec<Box<dyn DoctorCheck>>,
    participants: Vec<Box<dyn ContextParticipant>>,
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
        Self::open_with(path, clock, ids, actor, Vec::new())
    }

    /// `open` with the fixed participant list from kernel spec §3.14.
    /// The list does not change for the life of the handle.
    pub fn open_with(
        path: impl AsRef<Path>,
        clock: Box<dyn Clock>,
        ids: Box<dyn IdSource>,
        actor: &str,
        participants: Vec<Box<dyn ContextParticipant>>,
    ) -> Result<Self, Error> {
        Self::open_inner(path, clock, ids, actor, true, participants)
    }

    #[cfg(test)]
    pub(crate) fn open_observing_disabled_foreign_keys(
        path: impl AsRef<Path>,
        clock: Box<dyn Clock>,
        ids: Box<dyn IdSource>,
        actor: &str,
    ) -> Result<Self, Error> {
        Self::open_inner(path, clock, ids, actor, false, Vec::new())
    }

    fn open_inner(
        path: impl AsRef<Path>,
        clock: Box<dyn Clock>,
        ids: Box<dyn IdSource>,
        actor: &str,
        enable_foreign_keys: bool,
        participants: Vec<Box<dyn ContextParticipant>>,
    ) -> Result<Self, Error> {
        participants_ok(&participants)?;
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
        install_authorizer(&conn)?;
        let flag = read_fk(&conn)?;
        Ok(Self {
            conn,
            clock,
            ids,
            actor: actor.to_string(),
            writable: flag == 1,
            write_open: false,
            freezing: false,
            doctor_checks: Vec::new(),
            participants,
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
        };
        self.conn
            .query_row(sql, [id], |row| row.get(0))
            .optional()
            .map_err(|e| map_sql(e, SqlSite::Read))
    }

    pub fn posting_count(&self) -> Result<i64, Error> {
        self.scalar("SELECT count(*) FROM journal_postings")
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

    /// A context adds its own check. The runner does not import that context.
    /// Registry-scoped checks are stored and skipped; the application runner runs those.
    pub fn register_doctor_check(&mut self, check: Box<dyn DoctorCheck>) {
        self.doctor_checks.push(check);
    }

    /// Read the ledger-core checks and any registered single-database checks.
    /// Does not repair rows and does not clear a freeze. When a bugcheck finding
    /// other than an already-active freeze is present, persists that freeze in
    /// its own commit. A missing or stale `expected_revision` leaves the findings
    /// in the report and writes no freeze row.
    pub fn doctor(&mut self, expected_revision: Option<i64>) -> Result<DoctorReport, Error> {
        let findings = self.doctor_findings()?;
        let exit_status = exit_status(&findings);
        let mut report = DoctorReport {
            findings,
            exit_status,
            freeze: None,
            persist_error: None,
        };
        let Some(code) = report
            .findings
            .iter()
            .find(|finding| finding.severity == Severity::Bugcheck && persists_freeze(finding.code))
            .map(|finding| finding.code)
        else {
            return Ok(report);
        };
        if self.latest_freeze()?.is_some_and(|row| row.active == 1) {
            return Ok(report);
        }
        match self.record_write_freeze(expected_revision, code.as_str()) {
            Ok(outcome) => report.freeze = Some(outcome),
            Err(err) => report.persist_error = Some(err),
        }
        Ok(report)
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

    fn doctor_findings(&self) -> Result<Vec<Finding>, Error> {
        let mut findings = DoctorRunner::ledger_core().run(&self.conn)?;
        for check in &self.doctor_checks {
            if check.scope() != Scope::SingleDb {
                continue;
            }
            findings.extend(check.run(&self.conn)?);
        }
        Ok(findings)
    }

    fn blocking_bugcheck(&self) -> Result<(), Error> {
        // Clear refuses when a ledger-core bugcheck other than the active freeze
        // is present. Context checks registered on this handle are included.
        // Observation, evidence, recipe, and FX checks are not, until they register.
        for finding in self.doctor_findings()? {
            if finding.check_id == crate::v2::CHECK_FREEZE {
                continue;
            }
            if finding.severity != Severity::Bugcheck {
                continue;
            }
            return Err(Error {
                code: finding.code,
                class: finding.class,
                refs: finding.refs,
                ids: finding.ids,
                token: None,
            });
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

    pub(crate) fn downcast_participant<T: std::any::Any>(&self) -> Option<&T> {
        self.participants
            .iter()
            .find_map(|participant| participant.as_any().downcast_ref::<T>())
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

    /// Stage one current, non-reversal event and its postings (kernel spec §3.3).
    ///
    /// Lines only. Context links are staged by participants after this returns
    /// the stored ids (§3.14). `recorded_at` is the timestamp `begin_transaction`
    /// captured from the injected clock. Caller-supplied ULIDs are preserved.
    /// A reversal is `correct_event`, not this operation.
    pub fn append_event(&mut self, new: NewEvent) -> Result<String, Error> {
        self.append_event_with(new, ParticipantPayloads::new())
    }

    /// `append_event` with one opaque payload per participant id.
    pub fn append_event_with(
        &mut self,
        new: NewEvent,
        payloads: ParticipantPayloads,
    ) -> Result<String, Error> {
        let written = self.stage_new_event(new, None)?;
        self.dispatch_stage(
            StageOp::Append,
            &written.id,
            &written.line_ids,
            None,
            None,
            &payloads,
        )?;
        Ok(written.id)
    }

    /// `replacing_instance` is set only by `correct_event`, after that instance's
    /// current row was superseded in this same transaction. The retired-instance
    /// check does not apply to that replacement (kernel spec §3.9).
    fn stage_new_event(
        &mut self,
        new: NewEvent,
        replacing_instance: Option<&str>,
    ) -> Result<WrittenEvent, Error> {
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
        let half = self.recipe_half(&new)?;
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
            // ADR 0004: a recipe half's receivable or payable postings name a party.
            // U19 owns the full validator; this is the cheap write-path check.
            if half.is_some()
                && matches!(account.kind.as_str(), "receivable" | "payable")
                && posting.party_id.is_none()
            {
                return Err(self.fail(
                    Error::new(
                        Code::InvalidShape,
                        &["ADR 0004"],
                        vec![posting.ledger_account_id.clone()],
                    )
                    .with_token("recipe_party"),
                ));
            }
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
        if let Some(half) = &half {
            let replacing_this = replacing_instance == Some(half.recipe_instance_id.as_str());
            if !replacing_this {
                if let Some(existing) = self.current_half(&half.recipe_instance_id)? {
                    if self.same_recipe_payload(&new, half, &existing)? {
                        let seq = self.seq_of(&existing.commit_id)?;
                        self.staged.subjects.push(Subject::RecipeInstance {
                            recipe_instance_id: half.recipe_instance_id.clone(),
                            event_id: existing.id.clone(),
                            commit_id: existing.commit_id,
                            seq,
                        });
                        let line_ids = self.event_line_ids(&existing.id)?;
                        return Ok(WrittenEvent {
                            id: existing.id,
                            line_ids,
                        });
                    }
                    return Err(self.fail(Error::new(
                        Code::IdempotencyConflict,
                        &["ADR 0004"],
                        vec![half.recipe_instance_id.clone(), existing.id],
                    )));
                }
                if let Some(retired) = self.retired_event(&half.recipe_instance_id)? {
                    return Err(self.fail(Error::new(
                        Code::IdempotencyConflict,
                        &["ADR 0004"],
                        vec![half.recipe_instance_id.clone(), retired],
                    )));
                }
            }
        }
        self.write_event(&new, &prepared, half.as_ref())
    }

    /// Reverse a current event, set `superseded_by` once, then stage the replacement.
    ///
    /// Order is load-bearing: reversal insert, then the flag update, then the
    /// replacement. The replacement may use a different `effective_date`. A recipe
    /// half's replacement keeps the same instance and counterpart. `None` retires
    /// the event with no replacement. The reversal carries the target's effective
    /// date and no recipe-instance fields (ADR 0002, ADR 0004).
    pub fn correct_event(
        &mut self,
        target_id: &str,
        replacement: Option<NewEvent>,
    ) -> Result<Option<String>, Error> {
        self.correct_event_with(target_id, replacement, ParticipantPayloads::new())
    }

    /// `correct_event` with one opaque payload per participant id.
    /// Participants run once, after the replacement is staged.
    pub fn correct_event_with(
        &mut self,
        target_id: &str,
        replacement: Option<NewEvent>,
        payloads: ParticipantPayloads,
    ) -> Result<Option<String>, Error> {
        self.ensure_open()?;
        require_ulid(target_id).map_err(|e| self.fail(e))?;
        let Some(target) = self.load_target(target_id)? else {
            return Err(self.fail(Error::new(
                Code::UnknownTarget,
                &[],
                vec![target_id.to_string()],
            )));
        };
        if target.reverses_id.is_some() || target.superseded_by.is_some() {
            return Err(self.fail(Error::new(
                Code::ReversalTargetInvalid,
                &["ADR 0002"],
                vec![target.id.clone()],
            )));
        }
        if let (Some(instance), Some(next)) = (&target.recipe_instance_id, &replacement) {
            if next.recipe_instance_id.as_deref() != Some(instance.as_str())
                || next.counterpart_entity_id.as_deref() != target.counterpart_entity_id.as_deref()
            {
                return Err(self.fail(shape("recipe_fields")));
            }
        }
        let postings = self.event_postings(&target.id)?;
        if postings.len() < 2 {
            return Err(self.fail(Error::new(Code::InsufficientPostings, &["INV-002"], vec![])));
        }
        let reversal_id = self.allocate(None)?;
        let recorded_at = self.recorded_at.clone();
        let commit_id = self.commit_id.clone();
        self.exec(
            "INSERT INTO accounting_events(
                id, commit_id, recipe_id, recipe_version, effective_date, description, recorded_at, reverses_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, ?7)",
            params![
                reversal_id,
                commit_id,
                target.recipe_id,
                target.recipe_version,
                target.effective_date,
                recorded_at,
                target.id
            ],
        )?;
        self.staged.new_rows += 1;
        let mut reversal_lines = Vec::with_capacity(postings.len());
        for posting in &postings {
            let posting_id = self.allocate(None)?;
            reversal_lines.push(posting_id.clone());
            let side = if posting.side == "debit" {
                "credit"
            } else {
                "debit"
            };
            self.exec(
                "INSERT INTO journal_postings(
                    id, event_id, ledger_account_id, currency, side, amount_minor, party_id, memo
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    posting_id,
                    reversal_id,
                    posting.ledger_account_id,
                    posting.currency,
                    side,
                    posting.amount_minor,
                    posting.party_id,
                    posting.memo
                ],
            )?;
            self.staged.new_rows += 1;
        }
        self.exec(
            "UPDATE accounting_events SET superseded_by = ?1 WHERE id = ?2",
            params![reversal_id, target.id],
        )?;
        self.staged.correcting = true;
        self.staged.event_ids.push(reversal_id.clone());
        self.staged.kernel_events.push(StagedKernelEvent {
            event_id: reversal_id.clone(),
            line_ids: reversal_lines,
            reverses_id: Some(target.id.clone()),
        });
        self.staged.superseded_targets.push(target.id.clone());
        let (link_event, link_lines, result_id) = if let Some(next) = replacement {
            let replacing = target.recipe_instance_id.clone();
            let written = self.stage_new_event(next, replacing.as_deref())?;
            let id = written.id.clone();
            (written.id, written.line_ids, Some(id))
        } else {
            (reversal_id.clone(), Vec::new(), None)
        };
        self.dispatch_stage(
            StageOp::Correct,
            &link_event,
            &link_lines,
            Some(target.id),
            Some(reversal_id),
            &payloads,
        )?;
        Ok(result_id)
    }

    fn write_event(
        &mut self,
        new: &NewEvent,
        prepared: &[&NewPosting],
        half: Option<&RecipeHalf>,
    ) -> Result<WrittenEvent, Error> {
        let event_id = self.allocate(new.id.clone())?;
        if self.count(
            "SELECT count(*) FROM accounting_events WHERE id = ?1",
            &[event_id.as_str()],
        )? > 0
        {
            return Err(self.fail(Error::new(Code::DuplicateId, &[], vec![event_id])));
        }
        let recorded_at = self.recorded_at.clone();
        let commit_id = self.commit_id.clone();
        let (instance, counterpart) = match half {
            Some(half) => (
                Some(half.recipe_instance_id.as_str()),
                Some(half.counterpart_entity_id.as_str()),
            ),
            None => (None, None),
        };
        self.exec(
            "INSERT INTO accounting_events(
                id, commit_id, recipe_id, recipe_version, effective_date, description, recorded_at,
                recipe_instance_id, counterpart_entity_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event_id,
                commit_id,
                new.recipe_id,
                new.recipe_version,
                new.effective_date,
                new.description,
                recorded_at,
                instance,
                counterpart
            ],
        )?;
        self.staged.new_rows += 1;
        if let Some(half) = half {
            for (currency, amount) in &half.expectations {
                self.exec(
                    "INSERT INTO event_counterpart_expectations(
                        event_id, currency, expected_amount_minor
                     ) VALUES (?1, ?2, ?3)",
                    params![event_id, currency, amount],
                )?;
                self.staged.new_rows += 1;
            }
            self.staged.cross_entity = true;
        }
        let mut line_ids = Vec::with_capacity(prepared.len());
        for posting in prepared {
            let posting_id = self.allocate(posting.id.clone())?;
            line_ids.push(posting_id.clone());
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
        self.staged.kernel_events.push(StagedKernelEvent {
            event_id: event_id.clone(),
            line_ids: line_ids.clone(),
            reverses_id: None,
        });
        Ok(WrittenEvent {
            id: event_id,
            line_ids,
        })
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
        self.validate_participants()?;
        let kind = if self.staged.correcting {
            "correct"
        } else if self.staged.cross_entity {
            "cross_entity"
        } else if !self.staged.event_ids.is_empty() {
            "interpret"
        } else if self.staged.context_rows > 0 {
            "import"
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

    /// A context operation writes through the same `ContextTx` as `stage`.
    /// This is not the `stage` callback, so it may call kernel operations.
    pub(crate) fn with_participant<R>(
        &mut self,
        participant_id: &str,
        allow_insert: bool,
        body: impl FnOnce(&mut ContextTx<'_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        self.ensure_open()?;
        let owned = match self
            .kernel
            .participants
            .iter()
            .find(|participant| participant.id() == participant_id)
        {
            Some(participant) => participant
                .owned_tables()
                .iter()
                .map(|table| (*table).to_string())
                .collect::<Vec<_>>(),
            None => {
                return Err(self.fail(Error::new(
                    Code::UnknownTarget,
                    &["ADR 0011"],
                    vec![participant_id.to_string()],
                )))
            }
        };
        match self.run_participant(participant_id, &owned, allow_insert, body) {
            Ok((value, inserts)) => {
                self.staged.new_rows += inserts;
                self.staged.context_rows += inserts;
                Ok(value)
            }
            Err(err) => Err(self.fail(err)),
        }
    }

    pub(crate) fn note_subject(&mut self, subject: Subject) -> Result<(), Error> {
        self.ensure_open()?;
        self.staged.subjects.push(subject);
        Ok(())
    }

    fn dispatch_stage(
        &mut self,
        op: StageOp,
        event_id: &str,
        line_ids: &[String],
        target_event_id: Option<String>,
        reversal_event_id: Option<String>,
        payloads: &ParticipantPayloads,
    ) -> Result<(), Error> {
        if self.kernel.participants.is_empty() {
            return Ok(());
        }
        let staged = KernelStaged {
            op,
            commit_id: self.commit_id.clone(),
            recorded_at: self.recorded_at.clone(),
            event_id: event_id.to_string(),
            line_ids: line_ids.to_vec(),
            target_event_id,
            reversal_event_id,
        };
        let participants = std::mem::take(&mut self.kernel.participants);
        let mut error = None;
        for participant in &participants {
            if error.is_some() {
                break;
            }
            let id = participant.id().to_string();
            let owned = owned_tables(participant.as_ref());
            let payload = payloads.get(&id);
            match self.run_participant(&id, &owned, true, |tx| {
                participant.stage(tx, &staged, payload)
            }) {
                Ok((outcome, inserts)) => {
                    let counted = inserts.max(outcome.rows_staged);
                    self.staged.new_rows += counted;
                    self.staged.context_rows += counted;
                    self.staged.subjects.extend(outcome.subjects);
                }
                Err(err) => {
                    error = Some(admit_participant_error(participant.as_ref(), err));
                }
            }
        }
        self.kernel.participants = participants;
        if let Some(err) = error {
            return Err(self.fail(err));
        }
        Ok(())
    }

    fn validate_participants(&mut self) -> Result<(), Error> {
        if self.kernel.participants.is_empty() {
            return Ok(());
        }
        let events = self.staged.kernel_events.clone();
        let superseded = self.staged.superseded_targets.clone();
        let participants = std::mem::take(&mut self.kernel.participants);
        let mut error = None;
        for participant in &participants {
            if error.is_some() {
                break;
            }
            let id = participant.id().to_string();
            let owned = owned_tables(participant.as_ref());
            let own = self
                .staged
                .context_staged
                .get(&id)
                .cloned()
                .unwrap_or_default();
            let set = CommitStagedSet {
                events: events.clone(),
                superseded_targets: superseded.clone(),
                own_rows: own,
            };
            if let Err(err) =
                self.run_participant(&id, &owned, false, |tx| participant.validate(tx, &set))
            {
                error = Some(admit_participant_error(participant.as_ref(), err));
            }
        }
        self.kernel.participants = participants;
        if let Some(err) = error {
            return Err(self.fail(err));
        }
        Ok(())
    }

    fn run_participant<R>(
        &mut self,
        participant_id: &str,
        owned: &[String],
        allow_insert: bool,
        body: impl FnOnce(&mut ContextTx<'_>) -> Result<R, Error>,
    ) -> Result<(R, u32), Error> {
        let existing = self
            .staged
            .context_staged
            .get(participant_id)
            .cloned()
            .unwrap_or_default();
        let call = ContextCall {
            commit_id: &self.commit_id,
            recorded_at: &self.recorded_at,
            owned: owned.to_vec(),
            allow_insert,
        };
        let (value, inserts, rows) = in_context(&self.kernel.conn, call, existing, body)?;
        self.staged
            .context_staged
            .insert(participant_id.to_string(), rows);
        Ok((value, inserts))
    }

    fn event_line_ids(&mut self, event_id: &str) -> Result<Vec<String>, Error> {
        match load_line_ids(&self.kernel.conn, event_id) {
            Ok(ids) => Ok(ids),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    pub(crate) fn db(&self) -> &Connection {
        &self.kernel.conn
    }

    pub(crate) fn downcast_participant<T: std::any::Any>(&self) -> Option<&T> {
        self.kernel.downcast_participant()
    }

    pub(crate) fn ensure_open(&self) -> Result<(), Error> {
        if let Some(err) = &self.error {
            return Err(err.clone());
        }
        Ok(())
    }

    pub(crate) fn fail(&mut self, err: Error) -> Error {
        let expected = self.expected_revision;
        self.kernel.rollback();
        self.kernel.write_open = false;
        self.finished = true;
        self.error = Some(err.clone());
        self.kernel.maybe_persist_freeze(expected, &err);
        err
    }

    pub(crate) fn allocate(&mut self, supplied: Option<String>) -> Result<String, Error> {
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
                "SELECT account_type, kind, financial_account_id FROM ledger_accounts WHERE id = ?1",
                [id],
                |row| {
                    Ok(AccountSnap {
                        account_type: row.get(0)?,
                        kind: row.get(1)?,
                        financial_account_id: row.get(2)?,
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
        };
        Ok(self.count(sql, &[id])? > 0)
    }

    fn recipe_half(&mut self, new: &NewEvent) -> Result<Option<RecipeHalf>, Error> {
        let present = [
            new.recipe_instance_id.is_some(),
            new.counterpart_entity_id.is_some(),
        ];
        let fields = present.iter().filter(|flag| **flag).count();
        if fields == 0 {
            if !new.expectations.is_empty() {
                return Err(self.fail(shape("counterpart_expectations")));
            }
            return Ok(None);
        }
        if fields == 1 {
            return Err(self.fail(shape("recipe_fields")));
        }
        if new.expectations.is_empty() {
            return Err(self.fail(shape("counterpart_expectations")));
        }
        let recipe_instance_id = new.recipe_instance_id.clone().expect("present");
        let counterpart_entity_id = new.counterpart_entity_id.clone().expect("present");
        require_ulid(&recipe_instance_id).map_err(|e| self.fail(e))?;
        require_ulid(&counterpart_entity_id).map_err(|e| self.fail(e))?;
        let mut expectations = Vec::with_capacity(new.expectations.len());
        for row in &new.expectations {
            let bytes = row.currency.as_bytes();
            if bytes.len() != 3 || bytes.iter().any(|b| !b.is_ascii_uppercase()) {
                return Err(self.fail(shape("counterpart_expectations")));
            }
            require_positive(row.expected_amount_minor).map_err(|e| self.fail(e))?;
            if expectations
                .iter()
                .any(|(currency, _)| currency == &row.currency)
            {
                return Err(self.fail(shape("counterpart_expectations")));
            }
            expectations.push((row.currency.clone(), row.expected_amount_minor));
        }
        expectations.sort();
        let Some(entity_id) = self.singleton_entity()? else {
            return Err(self.fail(Error::new(
                Code::DatabaseUninitialized,
                &["ADR 0004"],
                vec![],
            )));
        };
        if counterpart_entity_id == entity_id {
            return Err(self.fail(shape("counterpart_self")));
        }
        Ok(Some(RecipeHalf {
            recipe_instance_id,
            counterpart_entity_id,
            expectations,
        }))
    }

    fn current_half(&mut self, instance: &str) -> Result<Option<StoredHalf>, Error> {
        self.load_half(
            "SELECT id, commit_id, recipe_id, recipe_version, effective_date, description,
                    recipe_instance_id, counterpart_entity_id
             FROM accounting_events
             WHERE recipe_instance_id = ?1 AND superseded_by IS NULL",
            instance,
        )
    }

    fn load_target(&mut self, id: &str) -> Result<Option<TargetEvent>, Error> {
        match self
            .kernel
            .conn
            .query_row(
                "SELECT id, recipe_id, recipe_version, effective_date,
                        recipe_instance_id, counterpart_entity_id, reverses_id, superseded_by
                 FROM accounting_events WHERE id = ?1",
                [id],
                |row| {
                    Ok(TargetEvent {
                        id: row.get(0)?,
                        recipe_id: row.get(1)?,
                        recipe_version: row.get(2)?,
                        effective_date: row.get(3)?,
                        recipe_instance_id: row.get(4)?,
                        counterpart_entity_id: row.get(5)?,
                        reverses_id: row.get(6)?,
                        superseded_by: row.get(7)?,
                    })
                },
            )
            .optional()
        {
            Ok(value) => Ok(value),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn load_half(&mut self, sql: &str, key: &str) -> Result<Option<StoredHalf>, Error> {
        match self.kernel.conn.query_row(sql, [key], |row| {
            Ok(StoredHalf {
                id: row.get(0)?,
                commit_id: row.get(1)?,
                recipe_id: row.get(2)?,
                recipe_version: row.get(3)?,
                effective_date: row.get(4)?,
                description: row.get(5)?,
                recipe_instance_id: row.get(6)?,
                counterpart_entity_id: row.get(7)?,
            })
        }) {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn retired_event(&mut self, instance: &str) -> Result<Option<String>, Error> {
        match self.kernel.conn.query_row(
            "SELECT id FROM accounting_events WHERE recipe_instance_id = ?1 LIMIT 1",
            [instance],
            |row| row.get(0),
        ) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(self.fail(map_sql(err, SqlSite::Read))),
        }
    }

    fn expectations_of(&mut self, event_id: &str) -> Result<Vec<(String, i64)>, Error> {
        let outcome = (|| {
            let mut stmt = self.kernel.conn.prepare(
                "SELECT currency, expected_amount_minor
                 FROM event_counterpart_expectations WHERE event_id = ?1 ORDER BY currency",
            )?;
            let rows = stmt.query_map([event_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })();
        outcome.map_err(|err| self.fail(map_sql(err, SqlSite::Read)))
    }

    fn event_postings(&mut self, event_id: &str) -> Result<Vec<StoredPosting>, Error> {
        let outcome = (|| {
            let mut stmt = self.kernel.conn.prepare(
                "SELECT ledger_account_id, currency, side, amount_minor, party_id, memo
                 FROM journal_postings WHERE event_id = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map([event_id], |row| {
                Ok(StoredPosting {
                    ledger_account_id: row.get(0)?,
                    currency: row.get(1)?,
                    side: row.get(2)?,
                    amount_minor: row.get(3)?,
                    party_id: row.get(4)?,
                    memo: row.get(5)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })();
        outcome.map_err(|err| self.fail(map_sql(err, SqlSite::Read)))
    }

    fn same_recipe_payload(
        &mut self,
        new: &NewEvent,
        half: &RecipeHalf,
        existing: &StoredHalf,
    ) -> Result<bool, Error> {
        if new.recipe_id != existing.recipe_id
            || new.recipe_version != existing.recipe_version
            || new.effective_date != existing.effective_date
            || new.description != existing.description
            || half.recipe_instance_id != existing.recipe_instance_id
            || half.counterpart_entity_id != existing.counterpart_entity_id
        {
            return Ok(false);
        }
        let stored_expectations = self.expectations_of(&existing.id)?;
        if stored_expectations != half.expectations {
            return Ok(false);
        }
        let stored = self.event_postings(&existing.id)?;
        if stored.len() != new.postings.len() {
            return Ok(false);
        }
        let mut left: Vec<_> = stored
            .iter()
            .map(|row| {
                (
                    row.ledger_account_id.clone(),
                    row.currency.clone(),
                    row.side.clone(),
                    row.amount_minor,
                    row.party_id.clone(),
                    row.memo.clone(),
                )
            })
            .collect();
        let mut right: Vec<_> = new
            .postings
            .iter()
            .map(|row| {
                (
                    row.ledger_account_id.clone(),
                    row.currency.clone(),
                    row.side.as_str().to_string(),
                    row.amount_minor,
                    row.party_id.clone(),
                    String::new(),
                )
            })
            .collect();
        left.sort();
        right.sort();
        Ok(left == right)
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

fn load_line_ids(conn: &Connection, event_id: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt =
        conn.prepare("SELECT id FROM journal_postings WHERE event_id = ?1 ORDER BY rowid")?;
    let rows = stmt.query_map([event_id], |row| row.get(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

fn owned_tables(participant: &dyn ContextParticipant) -> Vec<String> {
    participant
        .owned_tables()
        .iter()
        .map(|table| (*table).to_string())
        .collect()
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
