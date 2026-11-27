//! Doctor runner. The kernel owns the finding shape and the freeze policy.
//! Each context registers its own checks. U21 registers the ledger-core checks.
//!
//! The runner only reads. It does not repair rows and it does not clear a freeze.
//! `Kernel::doctor` persists a new freeze, in its own commit, when a bugcheck
//! finding is present and no freeze is already active (kernel spec §3.12).

mod ledger;

use rusqlite::Connection;

use crate::v2::error::{Code, Error};

pub use ledger::{
    CHECK_FOREIGN_KEYS, CHECK_FREEZE, CHECK_INTEGRITY, CHECK_SUPERSEDED_DRIFT, CHECK_UNBALANCED,
    CHECK_UNSEALED,
};

/// `bugcheck` fails doctor and can freeze the database.
/// `import` fails doctor and does not freeze it.
/// `warning` and `info` are reported and do not fail doctor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Bugcheck,
    Import,
    Warning,
    Info,
}

/// `single_db` runs on this handle. `registry` is for the application runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    SingleDb,
    Registry,
}

/// One check result. No amounts, names, memos, or payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub check_id: &'static str,
    pub severity: Severity,
    pub code: Code,
    pub class: crate::v2::error::Class,
    pub refs: Vec<String>,
    pub ids: Vec<String>,
}

/// Read-only check a context can register.
pub trait DoctorCheck {
    fn id(&self) -> &'static str;
    fn context(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn scope(&self) -> Scope;
    fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error>;
}

/// Ordered registry. Registration order is run order.
pub struct DoctorRunner {
    checks: Vec<Box<dyn DoctorCheck>>,
}

impl DoctorRunner {
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// The six ledger-core checks. Other contexts add their own with `register`.
    pub fn ledger_core() -> Self {
        let mut runner = Self::new();
        for check in ledger::checks() {
            runner.register(check);
        }
        runner
    }

    pub fn register(&mut self, check: Box<dyn DoctorCheck>) {
        self.checks.push(check);
    }

    /// Runs `single_db` checks. Registry-scoped checks are left for the application.
    pub fn run(&self, db: &Connection) -> Result<Vec<Finding>, Error> {
        let mut findings = Vec::new();
        for check in &self.checks {
            if check.scope() != Scope::SingleDb {
                continue;
            }
            findings.extend(check.run(db)?);
        }
        Ok(findings)
    }
}

impl Default for DoctorRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// 0 when every finding is `warning` or `info`, or there are none. Otherwise 1.
pub fn exit_status(findings: &[Finding]) -> i32 {
    if findings
        .iter()
        .any(|finding| matches!(finding.severity, Severity::Bugcheck | Severity::Import))
    {
        1
    } else {
        0
    }
}
