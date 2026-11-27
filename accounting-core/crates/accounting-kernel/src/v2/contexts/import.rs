//! Observation import (U22, ADR 0011).
//!
//! Stores already-keyed feed rows, links them to kernel lines by position, and
//! voids them by reversing those events in the same commit. `row_hash` is the
//! adapter's hash: a Monarch `Date` is the posted date when the adapter copies
//! it into `posted_date` (U23). This module does not read feed files.
//!
//! Evidence bytes are not a schema column. `put_evidence` writes them into the
//! directory given at registration, then stores `sha256` and size. A file with
//! no committed `evidence_objects` row is `OrphanEvidence`. A committed hash
//! whose file is missing or does not hash back is `EvidenceHashMismatch`.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::v2::doctor::{DoctorCheck, Finding, Scope, Severity};
use crate::v2::error::{Code, Error};
use crate::v2::participant::{
    CommitStagedSet, ContextParticipant, ContextTx, KernelStaged, ParticipantPayload,
    ParticipantPayloads, StageOutcome, Subject,
};
use crate::v2::sqlmap::{map_sql, SqlSite};
use crate::v2::store::{Kernel, Transaction};
use crate::v2::types::is_ulid;

pub const PARTICIPANT_ID: &str = "observation_import";
pub const CHECK_DOUBLE_CONSUMED: &str = "import.observation_double_consumed";
pub const CHECK_UNSEALED: &str = "import.unsealed_rows";
pub const CHECK_EVIDENCE_HASH: &str = "import.evidence_hash";
pub const CHECK_ORPHAN_EVIDENCE: &str = "import.orphan_evidence";

const CONTEXT: &str = "observation_import";

const TABLES: &[&str] = &[
    "feeds",
    "import_runs",
    "observations",
    "observation_voids",
    "posting_observations",
    "evidence_objects",
    "evidence_documents",
    "evidence_deletions",
    "evidence_links",
];

const CODES: &[Code] = &[
    Code::ImportConflict,
    Code::CorroborationMismatch,
    Code::ObservationMismatch,
    Code::ObservationDoubleConsumed,
    Code::VoidedObservation,
    Code::ObservationAlreadyVoided,
    Code::VoidWithoutReversal,
    Code::DissolveWithoutRepost,
    Code::OrphanEvidence,
    Code::EvidenceHashMismatch,
];

const UNSEALED_TABLES: &[&str] = &[
    "evidence_deletions",
    "evidence_links",
    "import_runs",
    "observations",
    "observation_voids",
    "posting_observations",
];

/// Registered at `open`. Doctor checks are registered separately so the kernel
/// runner stays free of import table names.
#[derive(Clone)]
pub struct ObservationImport {
    evidence_dir: PathBuf,
}

impl ObservationImport {
    pub fn new(evidence_dir: impl Into<PathBuf>) -> Self {
        Self {
            evidence_dir: evidence_dir.into(),
        }
    }

    pub fn participant(&self) -> Box<dyn ContextParticipant> {
        Box::new(self.clone())
    }

    /// Double-consumed, import-table unsealed rows, evidence hash, and orphan bytes.
    pub fn register_doctor_checks(&self, kernel: &mut Kernel) {
        kernel.register_doctor_check(Box::new(DoubleConsumedCheck));
        kernel.register_doctor_check(Box::new(UnsealedCheck));
        kernel.register_doctor_check(Box::new(EvidenceHashCheck {
            dir: self.evidence_dir.clone(),
        }));
        kernel.register_doctor_check(Box::new(OrphanEvidenceCheck {
            dir: self.evidence_dir.clone(),
        }));
    }
}

impl ContextParticipant for ObservationImport {
    fn id(&self) -> &str {
        PARTICIPANT_ID
    }

    fn owned_tables(&self) -> &'static [&'static str] {
        TABLES
    }

    fn declared_codes(&self) -> &'static [Code] {
        CODES
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn stage(
        &self,
        tx: &mut ContextTx<'_>,
        staged: &KernelStaged,
        payload: Option<&ParticipantPayload>,
    ) -> Result<StageOutcome, Error> {
        let Some(payload) = payload else {
            return Ok(StageOutcome::empty());
        };
        let Some(links) = payload.downcast_ref::<Vec<ObservationLink>>() else {
            return Err(Error::new(Code::InvalidShape, &[], vec![]));
        };
        let mut rows = 0u32;
        for link in links {
            let posting_id = staged.line_ids.get(link.line_index).ok_or_else(|| {
                Error::new(Code::InvalidShape, &[], vec![link.observation_id.clone()])
            })?;
            link_posting(tx, posting_id, &link.observation_id)?;
            rows += 1;
        }
        Ok(StageOutcome {
            rows_staged: rows,
            subjects: Vec::new(),
        })
    }

    fn validate(&self, tx: &ContextTx<'_>, set: &CommitStagedSet) -> Result<(), Error> {
        reject_double_consumed(tx, set)?;
        reject_void_without_reversal(tx)?;
        reject_missing_repost(tx)?;
        Ok(())
    }
}

/// A feed row. Re-saving the same id, kind, and name stages nothing.
pub struct NewFeed {
    pub id: Option<String>,
    pub kind: String,
    pub name: String,
}

/// One already-keyed source row. `row_hash` is 64 lowercase hex characters.
pub struct ObservationInput {
    pub id: Option<String>,
    pub external_id: String,
    pub financial_account_id: String,
    pub occurred_date: String,
    pub posted_date: Option<String>,
    pub direction: String,
    pub amount_minor: i64,
    pub currency: String,
    pub description: String,
    pub row_hash: String,
    pub payload_json: String,
    pub corroborates_id: Option<String>,
}

pub struct ImportBatch {
    pub feed_id: String,
    pub importer_id: String,
    pub importer_version: String,
    pub document_id: Option<String>,
    pub rows: Vec<ObservationInput>,
}

#[derive(Debug)]
pub struct ImportOutcome {
    pub import_run_id: Option<String>,
    pub inserted: u32,
    pub observation_ids: Vec<String>,
}

/// Link a staged line by its position in the event's line list, not by a caller-sent id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationLink {
    pub line_index: usize,
    pub observation_id: String,
}

pub struct NewEvidence {
    pub bytes: Vec<u8>,
    pub filename: Option<String>,
    pub media_type: Option<String>,
}

pub struct EvidenceRecord {
    pub hash: String,
    pub document_id: String,
}

pub enum EvidenceTarget {
    Event(String),
    Observation(String),
}

/// Payloads for `append_event_with` / `correct_event_with`.
pub fn observation_links(links: Vec<ObservationLink>) -> ParticipantPayloads {
    let mut payloads = ParticipantPayloads::new();
    if !links.is_empty() {
        payloads.insert(PARTICIPANT_ID, ParticipantPayload::new(links));
    }
    payloads
}

impl Transaction<'_> {
    pub fn ensure_feed(&mut self, feed: NewFeed) -> Result<String, Error> {
        self.ensure_open()?;
        if !matches!(
            feed.kind.as_str(),
            "monarch_csv" | "bankstmt_json" | "manual"
        ) {
            return Err(
                self.fail(Error::new(Code::InvalidShape, &[], vec![]).with_token("feed_kind"))
            );
        }
        if feed.name.is_empty() {
            return Err(self.fail(Error::new(Code::InvalidShape, &[], vec![])));
        }
        let id = self.allocate(feed.id.clone())?;
        let kind = feed.kind.clone();
        let name = feed.name.clone();
        self.with_participant(PARTICIPANT_ID, true, |tx| {
            if let Some((stored_kind, stored_name)) =
                tx.query_optional("SELECT kind, name FROM feeds WHERE id = ?1", [&id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
            {
                if stored_kind == kind && stored_name == name {
                    return Ok(());
                }
                return Err(Error::new(Code::DuplicateId, &[], vec![id.clone()]));
            }
            tx.insert_row(
                "feeds",
                &id,
                "INSERT INTO feeds(id, kind, name) VALUES (?1, ?2, ?3)",
                params![id, kind, name],
            )?;
            Ok(())
        })?;
        Ok(id)
    }

    pub fn import_observations(&mut self, batch: ImportBatch) -> Result<ImportOutcome, Error> {
        self.ensure_open()?;
        if !is_ulid(&batch.feed_id) {
            return Err(self.fail(Error::new(Code::InvalidId, &[], vec![])));
        }
        if batch.importer_id.is_empty() || batch.importer_version.is_empty() {
            return Err(self.fail(Error::new(Code::InvalidShape, &[], vec![])));
        }
        let mut allocated = Vec::with_capacity(batch.rows.len());
        for row in &batch.rows {
            allocated.push(self.allocate(row.id.clone())?);
        }
        let run_id = self.allocate(None)?;
        let work = self.with_participant(PARTICIPANT_ID, true, |tx| {
            stage_import(tx, &batch, &allocated, &run_id)
        })?;
        for subject in work.subjects {
            self.note_subject(subject)?;
        }
        Ok(work.outcome)
    }

    /// Tombstone the observation and reverse every current interpreting event.
    pub fn void_observation(&mut self, observation_id: &str, reason: &str) -> Result<(), Error> {
        self.ensure_open()?;
        if !is_ulid(observation_id) {
            return Err(self.fail(Error::new(Code::InvalidId, &[], vec![])));
        }
        if !matches!(reason, "duplicate" | "extraction_error" | "other") {
            return Err(
                self.fail(Error::new(Code::InvalidShape, &[], vec![]).with_token("void_reason"))
            );
        }
        let present = match scalar(
            self.db(),
            "SELECT count(*) FROM observations WHERE id = ?1",
            observation_id,
        ) {
            Ok(count) => count,
            Err(err) => return Err(self.fail(map_sql(err, SqlSite::Read))),
        };
        if present == 0 {
            return Err(self.fail(Error::new(
                Code::UnknownTarget,
                &[],
                vec![observation_id.to_string()],
            )));
        }
        let voided = match scalar(
            self.db(),
            "SELECT count(*) FROM observation_voids WHERE observation_id = ?1",
            observation_id,
        ) {
            Ok(count) => count,
            Err(err) => return Err(self.fail(map_sql(err, SqlSite::Read))),
        };
        if voided > 0 {
            return Err(self.fail(Error::new(
                Code::ObservationAlreadyVoided,
                &[],
                vec![observation_id.to_string()],
            )));
        }
        let events = match collect_strings(
            self.db(),
            "SELECT DISTINCT e.id
             FROM posting_observations po
             JOIN journal_postings p ON p.id = po.posting_id
             JOIN accounting_events e ON e.id = p.event_id
             WHERE po.observation_id = ?1
               AND e.superseded_by IS NULL
               AND e.reverses_id IS NULL
             ORDER BY e.id",
            observation_id,
        ) {
            Ok(events) => events,
            Err(err) => return Err(self.fail(map_sql(err, SqlSite::Read))),
        };
        for event_id in &events {
            self.correct_event(event_id, None)?;
        }
        let void_id = self.allocate(None)?;
        let observation_id = observation_id.to_string();
        let reason = reason.to_string();
        self.with_participant(PARTICIPANT_ID, true, |tx| {
            let commit_id = tx.commit_id().to_string();
            let recorded_at = tx.recorded_at().to_string();
            tx.insert_row(
                "observation_voids",
                &void_id,
                "INSERT INTO observation_voids(id, observation_id, reason, commit_id, recorded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![void_id, observation_id, reason, commit_id, recorded_at],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn put_evidence(&mut self, evidence: NewEvidence) -> Result<EvidenceRecord, Error> {
        self.ensure_open()?;
        let hash = sha256_hex(&evidence.bytes);
        let size = i64::try_from(evidence.bytes.len())
            .map_err(|_| self.fail(Error::new(Code::InvalidShape, &[], vec![])))?;
        let dir = evidence_dir_of(self)?;
        stage_bytes(&dir, &hash, &evidence.bytes).map_err(|err| self.fail(err))?;
        let document_id = self.allocate(None)?;
        let filename = evidence.filename.clone();
        let media_type = evidence.media_type.clone();
        self.with_participant(PARTICIPANT_ID, true, |tx| {
            let recorded_at = tx.recorded_at().to_string();
            let stored = tx.query_optional(
                "SELECT size_bytes FROM evidence_objects WHERE sha256 = ?1",
                [&hash],
                |row| row.get::<_, i64>(0),
            )?;
            if let Some(stored_size) = stored {
                if stored_size != size {
                    return Err(Error::new(Code::EvidenceHashMismatch, &[], vec![]));
                }
            } else {
                tx.insert_row(
                    "evidence_objects",
                    &hash,
                    "INSERT INTO evidence_objects(sha256, size_bytes, recorded_at)
                     VALUES (?1, ?2, ?3)",
                    params![hash, size, recorded_at],
                )?;
            }
            tx.insert_row(
                "evidence_documents",
                &document_id,
                "INSERT INTO evidence_documents(id, sha256, filename, media_type, recorded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![document_id, hash, filename, media_type, recorded_at],
            )?;
            Ok(())
        })?;
        Ok(EvidenceRecord { hash, document_id })
    }

    pub fn link_evidence(
        &mut self,
        document_id: &str,
        target: EvidenceTarget,
    ) -> Result<String, Error> {
        self.ensure_open()?;
        if !is_ulid(document_id) {
            return Err(self.fail(Error::new(Code::InvalidId, &[], vec![])));
        }
        let (event_id, observation_id, target_sql) = match &target {
            EvidenceTarget::Event(id) => {
                require_ulid_target(self, id)?;
                (
                    Some(id.clone()),
                    None,
                    "SELECT count(*) FROM accounting_events WHERE id = ?1",
                )
            }
            EvidenceTarget::Observation(id) => {
                require_ulid_target(self, id)?;
                (
                    None,
                    Some(id.clone()),
                    "SELECT count(*) FROM observations WHERE id = ?1",
                )
            }
        };
        let documents = match scalar(
            self.db(),
            "SELECT count(*) FROM evidence_documents WHERE id = ?1",
            document_id,
        ) {
            Ok(count) => count,
            Err(err) => return Err(self.fail(map_sql(err, SqlSite::Read))),
        };
        if documents == 0 {
            return Err(self.fail(Error::new(
                Code::UnknownTarget,
                &[],
                vec![document_id.to_string()],
            )));
        }
        let target_id = event_id
            .clone()
            .unwrap_or_else(|| observation_id.clone().unwrap_or_default());
        let targets = match scalar(self.db(), target_sql, &target_id) {
            Ok(count) => count,
            Err(err) => return Err(self.fail(map_sql(err, SqlSite::Read))),
        };
        if targets == 0 {
            return Err(self.fail(Error::new(Code::UnknownTarget, &[], vec![target_id])));
        }
        let link_id = self.allocate(None)?;
        let document_id = document_id.to_string();
        self.with_participant(PARTICIPANT_ID, true, |tx| {
            let commit_id = tx.commit_id().to_string();
            tx.insert_row(
                "evidence_links",
                &link_id,
                "INSERT INTO evidence_links(id, document_id, event_id, observation_id, commit_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![link_id, document_id, event_id, observation_id, commit_id],
            )?;
            Ok(())
        })?;
        Ok(link_id)
    }
}

struct ImportWork {
    outcome: ImportOutcome,
    subjects: Vec<Subject>,
}

struct StoredObservation {
    id: String,
    row_hash: String,
    commit_id: String,
    seq: i64,
}

fn stage_import(
    tx: &mut ContextTx<'_>,
    batch: &ImportBatch,
    allocated: &[String],
    run_id: &str,
) -> Result<ImportWork, Error> {
    let feed = tx.query_optional(
        "SELECT count(*) FROM feeds WHERE id = ?1",
        [&batch.feed_id],
        |row| row.get::<_, i64>(0),
    )?;
    if feed.unwrap_or(0) == 0 {
        return Err(Error::new(
            Code::UnknownTarget,
            &[],
            vec![batch.feed_id.clone()],
        ));
    }
    if let Some(document_id) = &batch.document_id {
        if !is_ulid(document_id) {
            return Err(Error::new(Code::InvalidId, &[], vec![]));
        }
        let found = tx.query_row(
            "SELECT count(*) FROM evidence_documents WHERE id = ?1",
            [document_id],
            |row| row.get::<_, i64>(0),
        )?;
        if found == 0 {
            return Err(Error::new(
                Code::UnknownTarget,
                &[],
                vec![document_id.clone()],
            ));
        }
    }
    let mut inserted_ids = Vec::new();
    let mut observation_ids = Vec::with_capacity(batch.rows.len());
    let mut subjects = Vec::new();
    for (row, new_id) in batch.rows.iter().zip(allocated.iter()) {
        check_row(tx, row, new_id)?;
        if let Some(stored) = stored_observation(tx, &batch.feed_id, &row.external_id)? {
            if stored.row_hash != row.row_hash {
                return Err(Error::new(
                    Code::ImportConflict,
                    &[],
                    vec![batch.feed_id.clone(), row.external_id.clone(), stored.id],
                ));
            }
            subjects.push(Subject::Observation {
                feed_id: batch.feed_id.clone(),
                external_id: row.external_id.clone(),
                commit_id: stored.commit_id,
                seq: stored.seq,
            });
            observation_ids.push(stored.id);
            continue;
        }
        inserted_ids.push((new_id.clone(), row));
        observation_ids.push(new_id.clone());
    }
    let import_run_id = if inserted_ids.is_empty() {
        None
    } else {
        let commit_id = tx.commit_id().to_string();
        let recorded_at = tx.recorded_at().to_string();
        tx.insert_row(
            "import_runs",
            run_id,
            "INSERT INTO import_runs(
                id, feed_id, document_id, importer_id, importer_version, commit_id, recorded_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run_id,
                batch.feed_id,
                batch.document_id,
                batch.importer_id,
                batch.importer_version,
                commit_id,
                recorded_at
            ],
        )?;
        for (id, row) in &inserted_ids {
            insert_observation(tx, id, &batch.feed_id, run_id, row)?;
        }
        Some(run_id.to_string())
    };
    Ok(ImportWork {
        outcome: ImportOutcome {
            import_run_id,
            inserted: u32::try_from(inserted_ids.len()).unwrap_or(u32::MAX),
            observation_ids,
        },
        subjects,
    })
}

fn check_row(tx: &ContextTx<'_>, row: &ObservationInput, new_id: &str) -> Result<(), Error> {
    if row.external_id.is_empty() || row.external_id.contains('\n') {
        return Err(Error::new(Code::InvalidShape, &[], vec![]));
    }
    if !is_hash(&row.row_hash) {
        return Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("hash"));
    }
    if !matches!(row.direction.as_str(), "inflow" | "outflow") {
        return Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("observation_direction"));
    }
    if row.amount_minor <= 0 {
        return Err(Error::new(Code::InvalidMoney, &[], vec![]));
    }
    if !is_ulid(&row.financial_account_id) {
        return Err(Error::new(Code::InvalidId, &[], vec![]));
    }
    if !date_ok(tx, &row.occurred_date)? {
        return Err(Error::new(Code::InvalidDate, &[], vec![]));
    }
    if let Some(posted) = &row.posted_date {
        if !date_ok(tx, posted)? {
            return Err(Error::new(Code::InvalidDate, &[], vec![]));
        }
    }
    let json_ok: i64 = tx.query_row("SELECT json_valid(?1)", [&row.payload_json], |r| r.get(0))?;
    if json_ok != 1 {
        return Err(Error::new(Code::InvalidShape, &[], vec![]));
    }
    let account_currency = tx.query_optional(
        "SELECT currency FROM financial_accounts WHERE id = ?1",
        [&row.financial_account_id],
        |r| r.get::<_, String>(0),
    )?;
    let Some(account_currency) = account_currency else {
        return Err(Error::new(
            Code::UnknownTarget,
            &[],
            vec![row.financial_account_id.clone()],
        ));
    };
    if account_currency != row.currency {
        return Err(Error::new(
            Code::CurrencyMismatch,
            &["INV-005"],
            vec![row.financial_account_id.clone()],
        ));
    }
    if let Some(corroborates) = &row.corroborates_id {
        if corroborates == new_id || !is_ulid(corroborates) {
            return Err(Error::new(
                Code::CorroborationMismatch,
                &["ADR 0007"],
                vec![corroborates.clone()],
            ));
        }
        let other = tx.query_optional(
            "SELECT financial_account_id FROM observations WHERE id = ?1",
            [corroborates],
            |r| r.get::<_, String>(0),
        )?;
        let Some(other_account) = other else {
            return Err(Error::new(
                Code::UnknownTarget,
                &[],
                vec![corroborates.clone()],
            ));
        };
        if other_account != row.financial_account_id {
            return Err(Error::new(
                Code::CorroborationMismatch,
                &["ADR 0007"],
                vec![corroborates.clone(), row.financial_account_id.clone()],
            ));
        }
    }
    Ok(())
}

fn stored_observation(
    tx: &ContextTx<'_>,
    feed_id: &str,
    external_id: &str,
) -> Result<Option<StoredObservation>, Error> {
    tx.query_optional(
        "SELECT o.id, o.row_hash, o.commit_id, c.seq
         FROM observations o
         JOIN commits c ON c.id = o.commit_id
         WHERE o.feed_id = ?1 AND o.external_id = ?2",
        params![feed_id, external_id],
        |row| {
            Ok(StoredObservation {
                id: row.get(0)?,
                row_hash: row.get(1)?,
                commit_id: row.get(2)?,
                seq: row.get(3)?,
            })
        },
    )
}

fn insert_observation(
    tx: &mut ContextTx<'_>,
    id: &str,
    feed_id: &str,
    run_id: &str,
    row: &ObservationInput,
) -> Result<(), Error> {
    let commit_id = tx.commit_id().to_string();
    let recorded_at = tx.recorded_at().to_string();
    tx.insert_row(
        "observations",
        id,
        "INSERT INTO observations(
            id, feed_id, external_id, import_run_id, financial_account_id,
            occurred_date, posted_date, direction, amount_minor, currency,
            description, row_hash, payload_json, corroborates_id, commit_id, recorded_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            id,
            feed_id,
            row.external_id,
            run_id,
            row.financial_account_id,
            row.occurred_date,
            row.posted_date,
            row.direction,
            row.amount_minor,
            row.currency,
            row.description,
            row.row_hash,
            row.payload_json,
            row.corroborates_id,
            commit_id,
            recorded_at
        ],
    )
}

fn link_posting(
    tx: &mut ContextTx<'_>,
    posting_id: &str,
    observation_id: &str,
) -> Result<(), Error> {
    if !is_ulid(observation_id) {
        return Err(Error::new(Code::InvalidId, &[], vec![]));
    }
    let posting = tx.query_optional(
        "SELECT a.financial_account_id, p.amount_minor, p.currency, p.side
         FROM journal_postings p
         JOIN ledger_accounts a ON a.id = p.ledger_account_id
         WHERE p.id = ?1",
        [posting_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let Some((account, amount, currency, side)) = posting else {
        return Err(Error::new(
            Code::UnknownTarget,
            &[],
            vec![posting_id.to_string()],
        ));
    };
    let observation = tx.query_optional(
        "SELECT financial_account_id, amount_minor, currency, direction,
                EXISTS(SELECT 1 FROM observation_voids v WHERE v.observation_id = o.id)
         FROM observations o WHERE o.id = ?1",
        [observation_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;
    let Some((obs_account, obs_amount, obs_currency, direction, voided)) = observation else {
        return Err(Error::new(
            Code::UnknownTarget,
            &[],
            vec![observation_id.to_string()],
        ));
    };
    if voided != 0 {
        return Err(Error::new(
            Code::VoidedObservation,
            &["ADR 0002"],
            vec![observation_id.to_string()],
        ));
    }
    let expected_side = if direction == "inflow" {
        "debit"
    } else {
        "credit"
    };
    if account.as_deref() != Some(obs_account.as_str())
        || amount != obs_amount
        || currency != obs_currency
        || side != expected_side
    {
        return Err(Error::new(
            Code::ObservationMismatch,
            &["INV-007"],
            vec![posting_id.to_string(), observation_id.to_string()],
        ));
    }
    tx.insert_row(
        "posting_observations",
        observation_id,
        "INSERT INTO posting_observations(posting_id, observation_id) VALUES (?1, ?2)",
        params![posting_id, observation_id],
    )
}

fn reject_double_consumed(tx: &ContextTx<'_>, set: &CommitStagedSet) -> Result<(), Error> {
    let mut seen = Vec::new();
    for row in &set.own_rows {
        if row.table != "posting_observations" || seen.contains(&row.row_id) {
            continue;
        }
        seen.push(row.row_id.clone());
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM posting_observations po
             JOIN journal_postings p ON p.id = po.posting_id
             JOIN accounting_events e ON e.id = p.event_id
             WHERE po.observation_id = ?1
               AND e.superseded_by IS NULL
               AND e.reverses_id IS NULL",
            [&row.row_id],
            |r| r.get(0),
        )?;
        if count > 1 {
            return Err(Error::new(
                Code::ObservationDoubleConsumed,
                &["INV-007"],
                vec![row.row_id.clone()],
            ));
        }
    }
    Ok(())
}

fn reject_void_without_reversal(tx: &ContextTx<'_>) -> Result<(), Error> {
    let stuck = tx.query_vec(
        "SELECT v.observation_id FROM observation_voids v
         WHERE v.commit_id = ?1
           AND EXISTS (
             SELECT 1 FROM posting_observations po
             JOIN journal_postings p ON p.id = po.posting_id
             JOIN accounting_events e ON e.id = p.event_id
             WHERE po.observation_id = v.observation_id
               AND e.superseded_by IS NULL
               AND e.reverses_id IS NULL
           )
         ORDER BY v.observation_id",
        [tx.commit_id()],
        |row| row.get::<_, String>(0),
    )?;
    if let Some(observation_id) = stuck.first() {
        return Err(Error::new(
            Code::VoidWithoutReversal,
            &["ADR 0002"],
            vec![observation_id.clone()],
        ));
    }
    Ok(())
}

fn reject_missing_repost(tx: &ContextTx<'_>) -> Result<(), Error> {
    let missing: i64 = tx.query_row(
        "SELECT count(*) FROM settlement_group_states s
         JOIN settlement_group_states prev
           ON prev.group_id = s.group_id AND prev.seq = s.seq - 1 AND prev.state = 'reconciled'
         JOIN settlement_postings sp ON sp.group_id = s.group_id
         JOIN posting_observations po ON po.posting_id = sp.posting_id
         WHERE s.commit_id = ?1 AND s.state = 'dissolved'
           AND NOT EXISTS (
             SELECT 1 FROM observation_voids v WHERE v.observation_id = po.observation_id
           )
           AND NOT EXISTS (
             SELECT 1 FROM posting_observations po2
             JOIN journal_postings p2 ON p2.id = po2.posting_id
             JOIN accounting_events e2 ON e2.id = p2.event_id
             WHERE po2.observation_id = po.observation_id
               AND e2.superseded_by IS NULL
               AND e2.reverses_id IS NULL
           )",
        [tx.commit_id()],
        |row| row.get(0),
    )?;
    if missing > 0 {
        return Err(Error::new(
            Code::DissolveWithoutRepost,
            &["ADR 0006"],
            vec![],
        ));
    }
    Ok(())
}

fn evidence_dir_of(tx: &Transaction<'_>) -> Result<PathBuf, Error> {
    tx.downcast_participant::<ObservationImport>()
        .map(|import| import.evidence_dir.clone())
        .ok_or_else(|| {
            Error::new(
                Code::UnknownTarget,
                &["ADR 0011"],
                vec![PARTICIPANT_ID.to_string()],
            )
        })
}

fn stage_bytes(dir: &Path, hash: &str, bytes: &[u8]) -> Result<(), Error> {
    std::fs::create_dir_all(dir).map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
    let path = dir.join(hash);
    if path.is_file() {
        let existing =
            std::fs::read(&path).map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
        if existing.as_slice() != bytes {
            return Err(Error::new(Code::EvidenceHashMismatch, &[], vec![]));
        }
        return Ok(());
    }
    let partial = dir.join(format!("{hash}.partial"));
    std::fs::write(&partial, bytes).map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
    std::fs::rename(&partial, &path).map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
    Ok(())
}

fn require_ulid_target(tx: &mut Transaction<'_>, id: &str) -> Result<(), Error> {
    if is_ulid(id) {
        Ok(())
    } else {
        Err(tx.fail(Error::new(Code::InvalidId, &[], vec![])))
    }
}

fn date_ok(tx: &ContextTx<'_>, value: &str) -> Result<bool, Error> {
    tx.query_row("SELECT date(?1) IS ?1", [value], |row| row.get(0))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_encode(Sha256::digest(bytes).as_slice())
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

fn scalar(conn: &Connection, sql: &str, arg: &str) -> Result<i64, rusqlite::Error> {
    conn.query_row(sql, [arg], |row| row.get(0))
}

fn collect_strings(
    conn: &Connection,
    sql: &str,
    arg: &str,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([arg], |row| row.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

struct DoubleConsumedCheck;
struct UnsealedCheck;

struct EvidenceHashCheck {
    dir: PathBuf,
}

struct OrphanEvidenceCheck {
    dir: PathBuf,
}

impl DoctorCheck for DoubleConsumedCheck {
    fn id(&self) -> &'static str {
        CHECK_DOUBLE_CONSUMED
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
            .prepare(
                "SELECT observation_id FROM doctor_observation_double_consumed
                 ORDER BY observation_id",
            )
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
        Ok(vec![finding(
            CHECK_DOUBLE_CONSUMED,
            Severity::Bugcheck,
            Code::DoctorDrift,
            &["INV-007"],
            ids,
        )])
    }
}

impl DoctorCheck for UnsealedCheck {
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
        let list = UNSEALED_TABLES
            .iter()
            .map(|table| format!("'{table}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT table_name, row_id, commit_id FROM doctor_unsealed_rows
             WHERE table_name IN ({list})
             ORDER BY table_name, row_id"
        );
        let mut stmt = db.prepare(&sql).map_err(read_err)?;
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
        Ok(vec![finding(
            CHECK_UNSEALED,
            Severity::Bugcheck,
            Code::UnsealedRows,
            &["PR-19"],
            ids,
        )])
    }
}

impl DoctorCheck for EvidenceHashCheck {
    fn id(&self) -> &'static str {
        CHECK_EVIDENCE_HASH
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
            .prepare("SELECT sha256, size_bytes FROM evidence_objects ORDER BY sha256")
            .map_err(read_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(read_err)?;
        let mut ids = Vec::new();
        let mut any = false;
        for row in rows {
            let (hash, size) = row.map_err(read_err)?;
            let bytes = std::fs::read(self.dir.join(&hash)).ok();
            let matches = bytes.as_ref().is_some_and(|bytes| {
                i64::try_from(bytes.len()).ok() == Some(size) && sha256_hex(bytes) == hash
            });
            if matches {
                continue;
            }
            any = true;
            let mut documents = db
                .prepare("SELECT id FROM evidence_documents WHERE sha256 = ?1 ORDER BY id")
                .map_err(read_err)?;
            let document_rows = documents
                .query_map([&hash], |doc| doc.get::<_, String>(0))
                .map_err(read_err)?;
            for document in document_rows {
                push_ulid(&mut ids, document.map_err(read_err)?);
            }
        }
        if !any {
            return Ok(Vec::new());
        }
        Ok(vec![finding(
            CHECK_EVIDENCE_HASH,
            Severity::Bugcheck,
            Code::EvidenceHashMismatch,
            &[],
            ids,
        )])
    }
}

impl DoctorCheck for OrphanEvidenceCheck {
    fn id(&self) -> &'static str {
        CHECK_ORPHAN_EVIDENCE
    }
    fn context(&self) -> &'static str {
        CONTEXT
    }
    fn severity(&self) -> Severity {
        Severity::Import
    }
    fn scope(&self) -> Scope {
        Scope::SingleDb
    }
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(Error::new(Code::StorageFault, &[], vec![])),
        };
        let mut orphan = false;
        for entry in entries {
            let entry = entry.map_err(|_| Error::new(Code::StorageFault, &[], vec![]))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !is_hash(name) {
                continue;
            }
            let count: i64 = db
                .query_row(
                    "SELECT count(*) FROM evidence_objects WHERE sha256 = ?1",
                    [name],
                    |row| row.get(0),
                )
                .map_err(read_err)?;
            if count == 0 {
                orphan = true;
            }
        }
        if !orphan {
            return Ok(Vec::new());
        }
        Ok(vec![finding(
            CHECK_ORPHAN_EVIDENCE,
            Severity::Import,
            Code::OrphanEvidence,
            &[],
            vec![],
        )])
    }
}

fn finding(
    check_id: &'static str,
    severity: Severity,
    code: Code,
    refs: &[&str],
    ids: Vec<String>,
) -> Finding {
    Finding {
        check_id,
        severity,
        code,
        class: code.class(),
        refs: refs.iter().map(|item| (*item).to_string()).collect(),
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
    match db.query_row(sql, [rowid], |row| row.get::<_, String>(0)) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(read_err(err)),
    }
}

fn subject_sql(table: &str) -> Option<&'static str> {
    Some(match table {
        "evidence_deletions" => "SELECT id FROM evidence_deletions WHERE rowid = ?1",
        "evidence_links" => "SELECT id FROM evidence_links WHERE rowid = ?1",
        "import_runs" => "SELECT id FROM import_runs WHERE rowid = ?1",
        "observations" => "SELECT id FROM observations WHERE rowid = ?1",
        "observation_voids" => "SELECT id FROM observation_voids WHERE rowid = ?1",
        "posting_observations" => "SELECT posting_id FROM posting_observations WHERE rowid = ?1",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::params;

    use crate::v2::error::Class;
    use crate::v2::types::{
        is_ulid, AccountType, FinancialKind, FixedClock, LedgerKind, PartyLabel, SequenceIds, Side,
        TrackingMode,
    };
    use crate::v2::{
        Code, Kernel, NewEntity, NewEvent, NewFinancialAccount, NewLedgerAccount, NewParty,
        NewPosting,
    };

    use super::*;

    const WHEN: &str = "2026-01-02T03:04:05Z";
    const MINOR: i64 = 1500;
    const POSTED: &str = "2026-01-02";

    struct Tmp {
        dir: PathBuf,
    }

    impl Tmp {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("ya-u22-{name}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self { dir }
        }

        fn path(&self) -> &Path {
            &self.dir
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    struct Books {
        card_fin: String,
        card: String,
        groceries: String,
    }

    fn uid(suffix: &str) -> String {
        let id = format!("01ARZ3NDEKTSV4RRFFQ69G{suffix}");
        assert!(is_ulid(&id), "fixture suffix {suffix}");
        id
    }

    fn hash_of(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    fn open_import(dir: &Path) -> Kernel {
        let evidence = dir.join("evidence");
        std::fs::create_dir_all(&evidence).expect("evidence dir");
        let import = ObservationImport::new(&evidence);
        let mut kernel = Kernel::open_with(
            dir.join("entity.sqlite"),
            Box::new(FixedClock {
                timestamp: WHEN.to_string(),
            }),
            Box::new(SequenceIds::new()),
            "u22",
            vec![import.participant()],
        )
        .expect("open");
        import.register_doctor_checks(&mut kernel);
        kernel
    }

    fn chart(kernel: &mut Kernel) -> Books {
        let books = Books {
            card_fin: uid("CARD"),
            card: uid("CRD1"),
            groceries: uid("GRC1"),
        };
        let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
        tx.create_entity(NewEntity {
            id: Some(uid("ENT1")),
            party_id: Some(uid("EPT1")),
            name: "Household".to_string(),
        })
        .expect("entity");
        tx.create_party(NewParty {
            id: Some(uid("ACME")),
            name: "Acme Corp".to_string(),
            label: PartyLabel::Employer,
            parent_id: None,
        })
        .expect("employer");
        tx.create_financial_account(NewFinancialAccount {
            id: Some(books.card_fin.clone()),
            name: "Example Card".to_string(),
            kind: FinancialKind::CreditCard,
            currency: "USD".to_string(),
            tracking_mode: TrackingMode::Transparent,
            institution: None,
            last4: Some("4242".to_string()),
        })
        .expect("financial");
        tx.create_ledger_account(NewLedgerAccount {
            id: Some(books.card.clone()),
            financial_account_id: Some(books.card_fin.clone()),
            account_type: AccountType::Liability,
            kind: LedgerKind::Card,
            parent_id: None,
            name: "Example Card".to_string(),
        })
        .expect("card");
        let food = uid("FD01");
        tx.create_ledger_account(NewLedgerAccount {
            id: Some(food.clone()),
            financial_account_id: None,
            account_type: AccountType::Expense,
            kind: LedgerKind::Category,
            parent_id: None,
            name: "Food".to_string(),
        })
        .expect("food");
        tx.create_ledger_account(NewLedgerAccount {
            id: Some(books.groceries.clone()),
            financial_account_id: None,
            account_type: AccountType::Expense,
            kind: LedgerKind::Category,
            parent_id: Some(food),
            name: "Groceries".to_string(),
        })
        .expect("groceries");
        tx.register_recipe("u22_probe", "1").expect("recipe");
        let sealed = tx.commit().expect("commit");
        assert_eq!(sealed.seq, 1);
        books
    }

    fn row(
        books: &Books,
        external_id: &str,
        posted: Option<&str>,
        hash: &str,
        currency: &str,
    ) -> ObservationInput {
        ObservationInput {
            id: None,
            external_id: external_id.to_string(),
            financial_account_id: books.card_fin.clone(),
            occurred_date: POSTED.to_string(),
            posted_date: posted.map(str::to_string),
            direction: "outflow".to_string(),
            amount_minor: MINOR,
            currency: currency.to_string(),
            description: "Synthetic row".to_string(),
            row_hash: hash.to_string(),
            payload_json: r#"{"source":"synthetic"}"#.to_string(),
            corroborates_id: None,
        }
    }

    fn batch(feed_id: &str, rows: Vec<ObservationInput>) -> ImportBatch {
        ImportBatch {
            feed_id: feed_id.to_string(),
            importer_id: "u22".to_string(),
            importer_version: "1".to_string(),
            document_id: None,
            rows,
        }
    }

    fn purchase(books: &Books, event: &str, debit: &str, credit: &str) -> NewEvent {
        NewEvent {
            id: Some(uid(event)),
            recipe_id: "u22_probe".to_string(),
            recipe_version: "1".to_string(),
            effective_date: POSTED.to_string(),
            description: String::new(),
            postings: vec![
                NewPosting {
                    id: Some(uid(debit)),
                    ledger_account_id: books.groceries.clone(),
                    currency: "USD".to_string(),
                    side: Side::Debit,
                    amount_minor: MINOR,
                    party_id: None,
                },
                NewPosting {
                    id: Some(uid(credit)),
                    ledger_account_id: books.card.clone(),
                    currency: "USD".to_string(),
                    side: Side::Credit,
                    amount_minor: MINOR,
                    party_id: None,
                },
            ],
            recipe_instance_id: None,
            counterpart_entity_id: None,
            expectations: Vec::new(),
        }
    }

    fn count(kernel: &Kernel, sql: &str) -> i64 {
        kernel
            .connection()
            .query_row(sql, [], |row| row.get(0))
            .expect("count")
    }

    fn review(kernel: &Kernel, id: &str) -> String {
        kernel
            .connection()
            .query_row(
                "SELECT review_state FROM observation_review_state WHERE observation_id = ?1",
                [id],
                |row| row.get(0),
            )
            .expect("review")
    }

    fn import_one(kernel: &mut Kernel, books: &Books, rev: i64) -> (String, String) {
        let mut tx = kernel.begin_transaction(Some(rev)).expect("begin");
        let feed = tx
            .ensure_feed(NewFeed {
                id: Some(uid("FE01")),
                kind: "monarch_csv".to_string(),
                name: "Synthetic Monarch".to_string(),
            })
            .expect("feed");
        let mut input = row(books, "monarch-1001", Some(POSTED), &hash_of(0x11), "USD");
        input.id = Some(uid("BS01"));
        let imported = tx
            .import_observations(batch(&feed, vec![input]))
            .expect("import");
        tx.commit().expect("commit");
        (feed, imported.observation_ids[0].clone())
    }

    #[test]
    fn reimport_adds_no_rows_and_a_changed_hash_conflicts() {
        let tmp = Tmp::new("reimport");
        let mut kernel = open_import(tmp.path());
        let books = chart(&mut kernel);
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        let feed = tx
            .ensure_feed(NewFeed {
                id: Some(uid("FE01")),
                kind: "monarch_csv".to_string(),
                name: "Synthetic Monarch".to_string(),
            })
            .expect("feed");
        let imported = tx
            .import_observations(batch(
                &feed,
                vec![
                    row(&books, "row-posted", Some(POSTED), &hash_of(0x11), "USD"),
                    row(&books, "row-open", None, &hash_of(0x22), "USD"),
                ],
            ))
            .expect("import");
        let sealed = tx.commit().expect("commit");
        assert_eq!(imported.inserted, 2);
        assert!(imported.import_run_id.is_some());
        assert_eq!(sealed.seq, 2);
        assert_eq!(
            kernel
                .commit_at_seq(sealed.seq)
                .expect("row")
                .expect("info")
                .kind,
            "import"
        );
        let posted: Option<String> = kernel
            .connection()
            .query_row(
                "SELECT posted_date FROM observations WHERE external_id = 'row-posted'",
                [],
                |row| row.get(0),
            )
            .expect("posted");
        assert_eq!(posted.as_deref(), Some(POSTED));
        let open_date: Option<String> = kernel
            .connection()
            .query_row(
                "SELECT posted_date FROM observations WHERE external_id = 'row-open'",
                [],
                |row| row.get(0),
            )
            .expect("open");
        assert_eq!(open_date, None);

        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        let again = tx
            .import_observations(batch(
                &feed,
                vec![
                    row(&books, "row-posted", Some(POSTED), &hash_of(0x11), "USD"),
                    row(&books, "row-open", None, &hash_of(0x22), "USD"),
                ],
            ))
            .expect("reimport");
        let noop = tx.commit().expect("noop");
        assert_eq!(again.inserted, 0);
        assert!(again.import_run_id.is_none());
        assert!(noop.noop);
        assert_eq!(noop.seq, 2);
        assert_eq!(noop.subjects.len(), 2);
        assert_eq!(count(&kernel, "SELECT count(*) FROM observations"), 2);
        assert_eq!(count(&kernel, "SELECT count(*) FROM import_runs"), 1);

        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        let err = tx
            .import_observations(batch(
                &feed,
                vec![row(
                    &books,
                    "row-posted",
                    Some(POSTED),
                    &hash_of(0x33),
                    "USD",
                )],
            ))
            .expect_err("conflict");
        assert_eq!(err.code, Code::ImportConflict);
        assert_eq!(err.class, Class::Import);
        assert!(err.ids.iter().any(|id| id == "row-posted"));
        assert!(!err.to_string().contains(&hash_of(0x33)));
        assert!(!err.to_string().contains("Synthetic row"));
        assert!(!err.to_string().contains("synthetic"));
        drop(tx);
        assert_eq!(count(&kernel, "SELECT count(*) FROM observations"), 2);

        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        let err = tx
            .import_observations(batch(
                &feed,
                vec![row(&books, "row-eur", Some(POSTED), &hash_of(0x44), "EUR")],
            ))
            .expect_err("currency");
        assert_eq!(err.code, Code::CurrencyMismatch);
        assert_eq!(err.class, Class::User);
        drop(tx);
        let report = kernel.doctor(Some(2)).expect("doctor");
        assert_eq!(report.exit_status, 0);
        assert!(report.freeze.is_none());
    }

    #[test]
    fn void_hides_the_observation_and_reverses_its_event() {
        let tmp = Tmp::new("void");
        let mut kernel = open_import(tmp.path());
        let books = chart(&mut kernel);
        let (_feed, observation) = import_one(&mut kernel, &books, 1);
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        tx.append_event_with(
            purchase(&books, "EV01", "PS01", "PS02"),
            observation_links(vec![ObservationLink {
                line_index: 1,
                observation_id: observation.clone(),
            }]),
        )
        .expect("link");
        tx.commit().expect("commit");
        assert_eq!(review(&kernel, &observation), "unreviewed");

        let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
        tx.void_observation(&observation, "duplicate")
            .expect("void");
        let sealed = tx.commit().expect("commit");
        assert_eq!(
            kernel
                .commit_at_seq(sealed.seq)
                .expect("row")
                .expect("info")
                .kind,
            "correct"
        );
        assert_eq!(review(&kernel, &observation), "voided");
        let superseded: Option<String> = kernel
            .connection()
            .query_row(
                "SELECT superseded_by FROM accounting_events WHERE id = ?1",
                [uid("EV01")],
                |row| row.get(0),
            )
            .expect("event");
        assert!(superseded.is_some());
        assert_eq!(
            count(&kernel, "SELECT count(*) FROM posting_observations"),
            1
        );

        let mut tx = kernel.begin_transaction(Some(4)).expect("begin");
        let err = tx
            .void_observation(&observation, "other")
            .expect_err("already");
        assert_eq!(err.code, Code::ObservationAlreadyVoided);
        drop(tx);

        let mut tx = kernel.begin_transaction(Some(4)).expect("begin");
        let err = tx
            .append_event_with(
                purchase(&books, "EV02", "PS03", "PS04"),
                observation_links(vec![ObservationLink {
                    line_index: 1,
                    observation_id: observation,
                }]),
            )
            .expect_err("voided link");
        assert_eq!(err.code, Code::VoidedObservation);
        drop(tx);
        assert_eq!(count(&kernel, "SELECT count(*) FROM accounting_events"), 2);
    }

    #[test]
    fn link_mismatch_and_void_without_reversal_roll_back() {
        let tmp = Tmp::new("rollback");
        let mut kernel = open_import(tmp.path());
        let books = chart(&mut kernel);
        let (_feed, observation) = import_one(&mut kernel, &books, 1);
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        let err = tx
            .append_event_with(
                purchase(&books, "EV01", "PS01", "PS02"),
                observation_links(vec![ObservationLink {
                    line_index: 0,
                    observation_id: observation.clone(),
                }]),
            )
            .expect_err("mismatch");
        assert_eq!(err.code, Code::ObservationMismatch);
        assert!(err.refs.iter().any(|r| r == "INV-007"));
        assert!(err.refs.iter().any(|r| r == "ADR 0011"));
        drop(tx);
        assert_eq!(count(&kernel, "SELECT count(*) FROM accounting_events"), 0);
        assert_eq!(
            count(&kernel, "SELECT count(*) FROM posting_observations"),
            0
        );

        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        tx.append_event_with(
            purchase(&books, "EV01", "PS01", "PS02"),
            observation_links(vec![ObservationLink {
                line_index: 1,
                observation_id: observation.clone(),
            }]),
        )
        .expect("link");
        tx.commit().expect("commit");

        let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
        tx.with_participant(PARTICIPANT_ID, true, |ctx| {
            let id = uid("VD01");
            let commit_id = ctx.commit_id().to_string();
            let recorded_at = ctx.recorded_at().to_string();
            ctx.insert_row(
                "observation_voids",
                &id,
                "INSERT INTO observation_voids(id, observation_id, reason, commit_id, recorded_at)
                 VALUES (?1, ?2, 'duplicate', ?3, ?4)",
                params![id, observation, commit_id, recorded_at],
            )?;
            Ok(())
        })
        .expect("stage void");
        let err = tx.commit().expect_err("void rule");
        assert_eq!(err.code, Code::VoidWithoutReversal);
        assert_ne!(err.code, Code::ValidatorMissed);
        assert!(err.refs.iter().any(|r| r == "ADR 0011"));
        assert_eq!(count(&kernel, "SELECT count(*) FROM observation_voids"), 0);
        let current: Option<String> = kernel
            .connection()
            .query_row(
                "SELECT superseded_by FROM accounting_events WHERE id = ?1",
                [uid("EV01")],
                |row| row.get(0),
            )
            .expect("event");
        assert_eq!(current, None);
    }

    #[test]
    fn second_link_is_rejected_and_planted_drift_freezes() {
        let tmp = Tmp::new("double");
        let mut kernel = open_import(tmp.path());
        let books = chart(&mut kernel);
        let (_feed, observation) = import_one(&mut kernel, &books, 1);
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        tx.append_event_with(
            purchase(&books, "EV01", "PS01", "PS02"),
            observation_links(vec![ObservationLink {
                line_index: 1,
                observation_id: observation.clone(),
            }]),
        )
        .expect("first");
        tx.commit().expect("commit");

        let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
        tx.append_event_with(
            purchase(&books, "EV02", "PS03", "PS04"),
            observation_links(vec![ObservationLink {
                line_index: 1,
                observation_id: observation.clone(),
            }]),
        )
        .expect("second stage");
        let err = tx.commit().expect_err("double");
        assert_eq!(err.code, Code::ObservationDoubleConsumed);
        assert!(err.refs.iter().any(|r| r == "INV-007"));
        assert_eq!(count(&kernel, "SELECT count(*) FROM accounting_events"), 1);

        // The open-commit trigger refuses a link after the posting's commit
        // exists, so the planted current interpretation has no commits row.
        kernel
            .connection()
            .execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        kernel
            .connection()
            .execute(
                "INSERT INTO accounting_events(
                    id, commit_id, recipe_id, recipe_version, effective_date, description, recorded_at
                 ) VALUES (?1, ?2, 'u22_probe', '1', '2026-01-02', '', ?3)",
                params![uid("EV03"), uid("MS01"), WHEN],
            )
            .expect("plant event");
        kernel
            .connection()
            .execute(
                "INSERT INTO journal_postings(
                    id, event_id, ledger_account_id, currency, side, amount_minor, memo
                 ) VALUES (?1, ?2, ?3, 'USD', 'credit', ?4, '')",
                params![uid("PS06"), uid("EV03"), books.card, MINOR],
            )
            .expect("plant posting");
        kernel
            .connection()
            .execute(
                "INSERT INTO posting_observations(posting_id, observation_id) VALUES (?1, ?2)",
                params![uid("PS06"), observation],
            )
            .expect("plant link");
        kernel
            .connection()
            .execute_batch("PRAGMA foreign_keys = ON")
            .expect("fk on");
        let rev = kernel.revision().expect("rev");
        let report = kernel.doctor(Some(rev)).expect("doctor");
        let finding = report
            .findings
            .iter()
            .find(|item| item.check_id == CHECK_DOUBLE_CONSUMED)
            .expect("double consumed");
        assert_eq!(finding.code, Code::DoctorDrift);
        assert_eq!(finding.refs, vec!["INV-007".to_string()]);
        assert!(finding.ids.contains(&observation));
        assert!(report.freeze.is_some());
    }

    #[test]
    fn correction_relinks_the_replacement_and_keeps_the_old_link() {
        let tmp = Tmp::new("relink");
        let mut kernel = open_import(tmp.path());
        let books = chart(&mut kernel);
        let (_feed, observation) = import_one(&mut kernel, &books, 1);
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        tx.append_event_with(
            purchase(&books, "EV01", "PS01", "PS02"),
            observation_links(vec![ObservationLink {
                line_index: 1,
                observation_id: observation.clone(),
            }]),
        )
        .expect("link");
        tx.commit().expect("commit");

        let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
        let replacement = tx
            .correct_event_with(
                &uid("EV01"),
                Some(purchase(&books, "EV02", "PS03", "PS04")),
                observation_links(vec![ObservationLink {
                    line_index: 1,
                    observation_id: observation.clone(),
                }]),
            )
            .expect("correct");
        tx.commit().expect("commit");
        assert_eq!(replacement.as_deref(), Some(uid("EV02").as_str()));
        assert_eq!(
            count(&kernel, "SELECT count(*) FROM posting_observations"),
            2
        );
        let current: i64 = kernel
            .connection()
            .query_row(
                "SELECT count(*) FROM posting_observations po
                 JOIN journal_postings p ON p.id = po.posting_id
                 JOIN accounting_events e ON e.id = p.event_id
                 WHERE po.observation_id = ?1
                   AND e.superseded_by IS NULL AND e.reverses_id IS NULL",
                [&observation],
                |row| row.get(0),
            )
            .expect("current");
        assert_eq!(current, 1);
        let old_link: i64 = kernel
            .connection()
            .query_row(
                "SELECT count(*) FROM posting_observations WHERE posting_id = ?1",
                [uid("PS02")],
                |row| row.get(0),
            )
            .expect("old");
        assert_eq!(old_link, 1);
        let report = kernel.doctor(Some(4)).expect("doctor");
        assert!(report
            .findings
            .iter()
            .all(|item| item.check_id != CHECK_DOUBLE_CONSUMED));
        assert_eq!(report.exit_status, 0);
    }

    #[test]
    fn evidence_hash_mismatch_freezes_and_orphan_bytes_do_not() {
        let tmp = Tmp::new("evidence");
        let mut kernel = open_import(tmp.path());
        let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
        let record = tx
            .put_evidence(NewEvidence {
                bytes: b"synthetic-receipt".to_vec(),
                filename: Some("receipt.txt".to_string()),
                media_type: Some("text/plain".to_string()),
            })
            .expect("put");
        tx.commit().expect("commit");
        let clean = kernel.doctor(Some(1)).expect("doctor");
        assert_eq!(clean.exit_status, 0);

        std::fs::write(tmp.path().join("evidence").join(&record.hash), b"tampered")
            .expect("tamper");
        let report = kernel.doctor(Some(1)).expect("doctor");
        let finding = report
            .findings
            .iter()
            .find(|item| item.check_id == CHECK_EVIDENCE_HASH)
            .expect("hash");
        assert_eq!(finding.code, Code::EvidenceHashMismatch);
        assert!(finding.ids.contains(&record.document_id));
        assert!(report.freeze.is_some());

        let tmp = Tmp::new("orphan");
        let mut kernel = open_import(tmp.path());
        let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
        tx.put_evidence(NewEvidence {
            bytes: b"staged-only".to_vec(),
            filename: None,
            media_type: None,
        })
        .expect("stage");
        drop(tx);
        assert_eq!(count(&kernel, "SELECT count(*) FROM evidence_objects"), 0);
        let report = kernel.doctor(Some(0)).expect("doctor");
        let finding = report
            .findings
            .iter()
            .find(|item| item.check_id == CHECK_ORPHAN_EVIDENCE)
            .expect("orphan");
        assert_eq!(finding.code, Code::OrphanEvidence);
        assert_eq!(finding.severity, crate::v2::Severity::Import);
        assert_eq!(report.exit_status, 1);
        assert!(report.freeze.is_none());
    }

    #[test]
    fn import_unsealed_rows_are_not_on_the_kernel_list() {
        let tmp = Tmp::new("unsealed");
        let mut kernel = open_import(tmp.path());
        let books = chart(&mut kernel);
        let (feed, _observation) = import_one(&mut kernel, &books, 1);
        let planted = uid("BS02");
        let missing = uid("MS01");
        kernel
            .connection()
            .execute_batch("PRAGMA foreign_keys = OFF")
            .expect("fk off");
        kernel
            .connection()
            .execute(
                "INSERT INTO observations(
                    id, feed_id, external_id, financial_account_id, occurred_date, direction,
                    amount_minor, currency, description, row_hash, payload_json, commit_id, recorded_at
                 ) VALUES (?1, ?2, 'planted', ?3, '2026-01-02', 'outflow', 1, 'USD', '', ?4, '{}', ?5, ?6)",
                params![
                    planted,
                    feed,
                    books.card_fin,
                    hash_of(0x55),
                    missing,
                    WHEN
                ],
            )
            .expect("plant");
        kernel
            .connection()
            .execute_batch("PRAGMA foreign_keys = ON")
            .expect("fk on");
        let rev = kernel.revision().expect("rev");
        let report = kernel.doctor(Some(rev)).expect("doctor");
        let import_finding = report
            .findings
            .iter()
            .find(|item| item.check_id == CHECK_UNSEALED)
            .expect("import unsealed");
        assert_eq!(import_finding.code, Code::UnsealedRows);
        assert!(import_finding.ids.contains(&planted));
        assert!(report.findings.iter().all(|item| {
            item.check_id != crate::v2::CHECK_UNSEALED || !item.ids.contains(&planted)
        }));
    }
}
