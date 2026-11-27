//! Context participant hook (kernel spec §3.14, ADR 0011).
//!
//! The kernel stages events and lines. A registered participant stages its own
//! rows in that same transaction and validates them before the `commits` row.
//! The kernel does not interpret those rows.

use std::any::Any;
use std::cell::Cell;
use std::collections::BTreeMap;

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{Connection, OptionalExtension, Params, Row};

use crate::v2::contexts::map_context_sql;
use crate::v2::error::{Code, Error};

/// One already-stored subject named by an idempotent no-op (§2.2).
///
/// `Observation` is supplied by the observation-import context. The kernel
/// only builds `RecipeInstance`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Observation {
        feed_id: String,
        external_id: String,
        commit_id: String,
        seq: i64,
    },
    RecipeInstance {
        recipe_instance_id: String,
        event_id: String,
        commit_id: String,
        seq: i64,
    },
}

/// Which kernel operation just staged its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOp {
    Append,
    Correct,
}

/// Stored ids from the kernel write that just finished.
///
/// `line_ids` are stored posting ids, in the caller's line order. On an
/// identical §3.9 retry they are the existing event's ids. On `correct` they
/// are the replacement's lines (empty when the correction has no replacement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelStaged {
    pub op: StageOp,
    pub commit_id: String,
    pub recorded_at: String,
    pub event_id: String,
    pub line_ids: Vec<String>,
    pub target_event_id: Option<String>,
    pub reversal_event_id: Option<String>,
}

/// Opaque per-participant payload. The kernel never downcasts it.
pub struct ParticipantPayload {
    value: Box<dyn Any + Send>,
}

impl ParticipantPayload {
    pub fn new<T: Any + Send>(value: T) -> Self {
        Self {
            value: Box::new(value),
        }
    }

    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.value.downcast_ref()
    }
}

impl std::fmt::Debug for ParticipantPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ParticipantPayload")
    }
}

/// Payloads attached to one `append_event` or `correct`, keyed by participant id.
#[derive(Default)]
pub struct ParticipantPayloads {
    inner: BTreeMap<String, ParticipantPayload>,
}

impl ParticipantPayloads {
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, participant_id: impl Into<String>, payload: ParticipantPayload) {
        self.inner.insert(participant_id.into(), payload);
    }

    pub(crate) fn get(&self, participant_id: &str) -> Option<&ParticipantPayload> {
        self.inner.get(participant_id)
    }
}

/// What `stage` wrote. `subjects` are included in a §2.2 no-op result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageOutcome {
    pub rows_staged: u32,
    pub subjects: Vec<Subject>,
}

impl StageOutcome {
    pub fn empty() -> Self {
        Self {
            rows_staged: 0,
            subjects: Vec::new(),
        }
    }
}

/// One context row staged in this transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedContextRow {
    pub table: String,
    pub row_id: String,
}

/// A kernel event staged in this transaction, including a reversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedKernelEvent {
    pub event_id: String,
    pub line_ids: Vec<String>,
    pub reverses_id: Option<String>,
}

/// The set `validate` sees: kernel rows from this transaction, plus this
/// participant's own staged rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitStagedSet {
    pub events: Vec<StagedKernelEvent>,
    pub superseded_targets: Vec<String>,
    pub own_rows: Vec<StagedContextRow>,
}

/// In-process context registered at `open`. Called in registration order.
pub trait ContextParticipant: Send {
    fn id(&self) -> &str;
    fn owned_tables(&self) -> &'static [&'static str];
    fn declared_codes(&self) -> &'static [Code];
    /// Recovers the concrete participant. The kernel does not inspect the value.
    fn as_any(&self) -> &dyn Any;
    fn stage(
        &self,
        tx: &mut ContextTx<'_>,
        staged: &KernelStaged,
        payload: Option<&ParticipantPayload>,
    ) -> Result<StageOutcome, Error>;
    fn validate(&self, tx: &ContextTx<'_>, set: &CommitStagedSet) -> Result<(), Error>;
}

const SHARED_CODES: &[Code] = &[
    Code::CurrencyMismatch,
    Code::InvalidShape,
    Code::UnknownTarget,
    Code::DuplicateId,
    Code::InvalidId,
    Code::InvalidDate,
];

pub(crate) fn admit_participant_error(participant: &dyn ContextParticipant, err: Error) -> Error {
    let declared = participant.declared_codes().contains(&err.code);
    let shared = SHARED_CODES.contains(&err.code);
    if !declared && !shared {
        return Error::new(Code::UnmappedStorageError, &["ADR 0011"], vec![]);
    }
    let mut refs = err.refs;
    if !refs.iter().any(|r| r == "ADR 0011") {
        refs.push("ADR 0011".to_string());
    }
    Error {
        code: err.code,
        class: err.class,
        refs,
        ids: err.ids,
        token: err.token,
    }
}

pub(crate) fn participants_ok(participants: &[Box<dyn ContextParticipant>]) -> Result<(), Error> {
    let mut ids: Vec<&str> = Vec::new();
    let mut tables: Vec<&str> = Vec::new();
    for participant in participants {
        let id = participant.id();
        if id.is_empty() || id.contains('\n') || ids.contains(&id) {
            return Err(Error::new(
                Code::InvalidShape,
                &["ADR 0011"],
                vec![id.to_string()],
            ));
        }
        ids.push(id);
        if participant.owned_tables().is_empty() {
            return Err(Error::new(
                Code::InvalidShape,
                &["ADR 0011"],
                vec![id.to_string()],
            ));
        }
        for table in participant.owned_tables() {
            if tables.contains(table) {
                return Err(Error::new(
                    Code::InvalidShape,
                    &["ADR 0011"],
                    vec![id.to_string(), (*table).to_string()],
                ));
            }
            tables.push(table);
        }
    }
    Ok(())
}

struct Authz {
    owned: Vec<String>,
    allow_insert: bool,
}

thread_local! {
    static AUTHZ: Cell<*const Authz> = const { Cell::new(std::ptr::null()) };
}

struct AuthzGuard;

impl AuthzGuard {
    fn arm(authz: &Authz) -> Self {
        AUTHZ.with(|cell| cell.set(authz));
        Self
    }
}

impl Drop for AuthzGuard {
    fn drop(&mut self) {
        AUTHZ.with(|cell| cell.set(std::ptr::null()));
    }
}

/// Install the authorizer once. With no participant call in progress every
/// action is allowed, so kernel writes are unchanged.
pub(crate) fn install_authorizer(conn: &Connection) -> Result<(), Error> {
    conn.authorizer(Some(authorize));
    Ok(())
}

fn authorize(ctx: AuthContext<'_>) -> Authorization {
    AUTHZ.with(|cell| {
        let ptr = cell.get();
        if ptr.is_null() {
            return Authorization::Allow;
        }
        // SAFETY: `AuthzGuard` keeps `ptr` pointed at the `Authz` on the stack
        // of the participant call until that call returns. SQLite invokes this
        // hook synchronously on this thread while the guard is armed.
        let authz = unsafe { &*ptr };
        decide(authz, &ctx.action)
    })
}

fn decide(authz: &Authz, action: &AuthAction<'_>) -> Authorization {
    match action {
        AuthAction::Select
        | AuthAction::Read { .. }
        | AuthAction::Function { .. }
        | AuthAction::Recursive => Authorization::Allow,
        AuthAction::Insert { table_name } if authz.allow_insert && owns(authz, table_name) => {
            Authorization::Allow
        }
        _ => Authorization::Deny,
    }
}

fn owns(authz: &Authz, table: &str) -> bool {
    authz
        .owned
        .iter()
        .any(|owned| owned.eq_ignore_ascii_case(table))
}

/// Table-scoped handle for one participant call. Valid only for that call.
pub struct ContextTx<'a> {
    conn: &'a Connection,
    commit_id: &'a str,
    recorded_at: &'a str,
    allow_insert: bool,
    inserts: &'a Cell<u32>,
    rows: &'a mut Vec<StagedContextRow>,
}

impl ContextTx<'_> {
    pub fn commit_id(&self) -> &str {
        self.commit_id
    }

    pub fn recorded_at(&self) -> &str {
        self.recorded_at
    }

    pub fn allow_insert(&self) -> bool {
        self.allow_insert
    }

    /// INSERT into one of this participant's tables and remember the row.
    pub fn insert_row<P: Params>(
        &mut self,
        table: &str,
        row_id: &str,
        sql: &str,
        params: P,
    ) -> Result<(), Error> {
        self.execute(sql, params)?;
        self.rows.push(StagedContextRow {
            table: table.to_string(),
            row_id: row_id.to_string(),
        });
        Ok(())
    }

    pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<(), Error> {
        self.conn.execute(sql, params).map_err(map_context_sql)?;
        if is_insert(sql) {
            self.inserts.set(self.inserts.get().saturating_add(1));
        }
        Ok(())
    }

    pub fn query_row<T, P, F>(&self, sql: &str, params: P, map: F) -> Result<T, Error>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    {
        self.conn
            .query_row(sql, params, map)
            .map_err(map_context_sql)
    }

    pub fn query_optional<T, P, F>(&self, sql: &str, params: P, map: F) -> Result<Option<T>, Error>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    {
        self.conn
            .query_row(sql, params, map)
            .optional()
            .map_err(map_context_sql)
    }

    pub fn query_vec<T, P, F>(&self, sql: &str, params: P, mut map: F) -> Result<Vec<T>, Error>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
    {
        let mut stmt = self.conn.prepare(sql).map_err(map_context_sql)?;
        let rows = stmt
            .query_map(params, |row| map(row))
            .map_err(map_context_sql)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_context_sql)?);
        }
        Ok(out)
    }
}

fn is_insert(sql: &str) -> bool {
    sql.trim_start()
        .get(..6)
        .is_some_and(|head| head.eq_ignore_ascii_case("insert"))
}

pub(crate) struct ContextCall<'a> {
    pub commit_id: &'a str,
    pub recorded_at: &'a str,
    pub owned: Vec<String>,
    pub allow_insert: bool,
}

/// Run `body` with the authorizer limited to `call.owned`.
pub(crate) fn in_context<R>(
    conn: &Connection,
    call: ContextCall<'_>,
    mut rows: Vec<StagedContextRow>,
    body: impl FnOnce(&mut ContextTx<'_>) -> Result<R, Error>,
) -> Result<(R, u32, Vec<StagedContextRow>), Error> {
    let authz = Authz {
        owned: call.owned,
        allow_insert: call.allow_insert,
    };
    let _guard = AuthzGuard::arm(&authz);
    let inserts = Cell::new(0u32);
    let result = {
        let mut tx = ContextTx {
            conn,
            commit_id: call.commit_id,
            recorded_at: call.recorded_at,
            allow_insert: call.allow_insert,
            inserts: &inserts,
            rows: &mut rows,
        };
        body(&mut tx)
    };
    drop(_guard);
    result.map(|value| (value, inserts.get(), rows))
}
