//! Pure Domain Value Objects, Aggregate Roots, and Port Traits for Double-Entry Bookkeeping.
//!
//! This module contains zero dependencies on database engines or SQL drivers.
//! Storage is abstracted behind the `LedgerStore` trait.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_deterministic_id(prefix: &str) -> String {
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{seq:08x}")
}

// =============================================================================
// DOMAIN ERRORS (NO INFRASTRUCTURE LEAKS)
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelError {
    InvalidCurrency(String),
    NonPositiveAmount(i64),
    ArithmeticOverflow,
    InsufficientPostings(usize),
    UnbalancedPartitions(BTreeMap<(String, Currency), i64>),
    MissingInterEntityAccounts { target_entity_id: String },
    UnknownLedgerAccount(String),
    UnknownEntityPartition { entity_id: String, currency: String },
    EquationViolation(EquationSnapshot),
    Storage(String),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCurrency(c) => {
                write!(f, "Currency must be a 3-letter uppercase ISO code, got {c:?}")
            }
            Self::NonPositiveAmount(a) => {
                write!(f, "Posting Money magnitude must be > 0, got {a}")
            }
            Self::ArithmeticOverflow => write!(f, "Checked integer arithmetic overflow"),
            Self::InsufficientPostings(n) => {
                write!(f, "INV-001: A JournalEntry must contain at least 2 postings (got {n})")
            }
            Self::UnbalancedPartitions(map) => {
                write!(f, "INV-001: Unbalanced (entity, currency) partitions: {map:?}")
            }
            Self::MissingInterEntityAccounts { target_entity_id } => write!(
                f,
                "Cross-entity split for {target_entity_id} requires paired due_from and due_to accounts"
            ),
            Self::UnknownLedgerAccount(id) => write!(f, "Unknown ledger_account_id: {id}"),
            Self::UnknownEntityPartition { entity_id, currency } => {
                write!(f, "No ledger accounts found for ({entity_id}, {currency})")
            }
            Self::EquationViolation(snap) => {
                write!(f, "Accounting equation violated: {snap:?}")
            }
            Self::Storage(msg) => write!(f, "Storage failure: {msg}"),
        }
    }
}

impl std::error::Error for KernelError {}

// =============================================================================
// 1. CORE VALUE OBJECTS (IMMUTABLE PRIMITIVES)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PostingSide {
    Debit,
    Credit,
}

impl PostingSide {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }
}

/// The 5 fundamental elements of the expanded accounting equation:
/// `Assets + Expenses == Liabilities + Equity + Revenue`
/// Rearranged into normal balances:
/// `Assets == Liabilities + Equity + (Revenue - Expenses)`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountElement {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

impl AccountElement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Liability => "liability",
            Self::Equity => "equity",
            Self::Revenue => "revenue",
            Self::Expense => "expense",
        }
    }

    pub const fn normal_side(self) -> PostingSide {
        match self {
            Self::Asset | Self::Expense => PostingSide::Debit,
            Self::Liability | Self::Equity | Self::Revenue => PostingSide::Credit,
        }
    }

    pub const fn signed_delta(self, side: PostingSide, amount_minor: i64) -> i64 {
        match (self.normal_side(), side) {
            (PostingSide::Debit, PostingSide::Debit)
            | (PostingSide::Credit, PostingSide::Credit) => amount_minor,
            _ => -amount_minor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountRole {
    Standard,
    Clearing,
    Contra,
}

impl AccountRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Clearing => "clearing",
            Self::Contra => "contra",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityVisibility {
    Transparent,
    Opaque,
}

impl EntityVisibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transparent => "transparent",
            Self::Opaque => "opaque",
        }
    }
}

/// 3-letter uppercase ASCII ISO-4217 currency code.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Currency([u8; 3]);

impl Currency {
    pub fn parse(code: &str) -> Result<Self, KernelError> {
        let bytes = code.as_bytes();
        if bytes.len() == 3 && bytes.iter().all(|b| b.is_ascii_uppercase()) {
            Ok(Self([bytes[0], bytes[1], bytes[2]]))
        } else {
            Err(KernelError::InvalidCurrency(code.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("validated ASCII uppercase")
    }
}

impl fmt::Debug for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Currency({})", self.as_str())
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Exact integer minor-unit monetary value with ISO-4217 currency guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    amount_minor: i64,
    currency: Currency,
}

impl Money {
    pub fn new(amount_minor: i64, currency: &str) -> Result<Self, KernelError> {
        if amount_minor <= 0 {
            return Err(KernelError::NonPositiveAmount(amount_minor));
        }
        let currency = Currency::parse(currency)?;
        Ok(Self {
            amount_minor,
            currency,
        })
    }

    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }

    pub const fn currency(&self) -> Currency {
        self.currency
    }
}

/// Single immutable debit or credit line inside a `JournalEntryDraft` aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingPrimitive {
    pub id: String,
    pub ledger_account_id: String,
    pub entity_id: String,
    pub money: Money,
    pub side: PostingSide,
    pub memo: String,
}

impl PostingPrimitive {
    pub fn new(
        ledger_account_id: impl Into<String>,
        entity_id: impl Into<String>,
        money: Money,
        side: PostingSide,
        memo: impl Into<String>,
    ) -> Self {
        Self {
            id: next_deterministic_id("pst"),
            ledger_account_id: ledger_account_id.into(),
            entity_id: entity_id.into(),
            money,
            side,
            memo: memo.into(),
        }
    }
}

// =============================================================================
// 2. CORE AGGREGATE ROOTS (PURE DOUBLE-ENTRY KERNEL)
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntryDraft {
    pub id: String,
    pub posting_date: String,
    pub effective_date: Option<String>,
    pub description: String,
    pub replaces_entry_id: Option<String>,
    pub postings: Vec<PostingPrimitive>,
}

impl JournalEntryDraft {
    pub fn new(
        posting_date: impl Into<String>,
        description: impl Into<String>,
        postings: Vec<PostingPrimitive>,
    ) -> Self {
        Self {
            id: next_deterministic_id("je"),
            posting_date: posting_date.into(),
            effective_date: None,
            description: description.into(),
            replaces_entry_id: None,
            postings,
        }
    }

    /// Enforces INV-001: Every `(entity_id, currency)` partition must satisfy
    /// `SUM(debits) == SUM(credits)`.
    pub fn validate_trial_balance(&self) -> Result<BTreeMap<(String, Currency), i64>, KernelError> {
        if self.postings.len() < 2 {
            return Err(KernelError::InsufficientPostings(self.postings.len()));
        }

        let mut net_by_partition: BTreeMap<(String, Currency), i64> = BTreeMap::new();
        for p in &self.postings {
            let key = (p.entity_id.clone(), p.money.currency());
            let signed = match p.side {
                PostingSide::Debit => p.money.amount_minor(),
                PostingSide::Credit => -p.money.amount_minor(),
            };
            let current = net_by_partition.entry(key).or_insert(0);
            *current = current
                .checked_add(signed)
                .ok_or(KernelError::ArithmeticOverflow)?;
        }

        let unbalanced: BTreeMap<(String, Currency), i64> = net_by_partition
            .iter()
            .filter(|(_, &net)| net != 0)
            .map(|(k, &v)| (k.clone(), v))
            .collect();

        if !unbalanced.is_empty() {
            return Err(KernelError::UnbalancedPartitions(unbalanced));
        }

        Ok(net_by_partition)
    }
}

/// Read-model verification of `Assets == Liabilities + Equity + (Revenue - Expenses)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquationSnapshot {
    pub entity_id: String,
    pub currency: String,
    pub trial_debits_minor: i64,
    pub trial_credits_minor: i64,
    pub assets_minor: i64,
    pub liabilities_minor: i64,
    pub equity_paid_in_minor: i64,
    pub revenue_minor: i64,
    pub expenses_minor: i64,
    pub total_equity_minor: i64,
    pub equation_discrepancy_minor: i64,
}

impl EquationSnapshot {
    pub const fn is_balanced(&self) -> bool {
        self.trial_debits_minor == self.trial_credits_minor
            && self.assets_minor == (self.liabilities_minor + self.total_equity_minor)
            && self.equation_discrepancy_minor == 0
    }
}

// =============================================================================
// 3. THE PRIMARY DOMAIN PORT (TRAIT LEDGERSTORE)
// =============================================================================

/// Primary Port defining the abstract double-entry bookkeeping engine capabilities.
/// Outer application rings (recipes, sub-ledgers, reports) communicate ONLY via this trait.
pub trait LedgerStore {
    /// Registers an accounting entity (transparent with full books, or opaque external party).
    fn register_entity(
        &mut self,
        entity_id: &str,
        name: &str,
        visibility: EntityVisibility,
    ) -> Result<(), KernelError>;

    /// Creates an engine-managed ledger account classified into one of the 5 elements.
    fn create_ledger_account(
        &mut self,
        account_id: &str,
        entity_id: &str,
        name: &str,
        currency: &str,
        element: AccountElement,
        role: AccountRole,
    ) -> Result<(), KernelError>;

    /// Atomically validates in-memory, posts, and seals a JournalEntryDraft.
    /// Returns the assigned entry ID.
    fn post_entry(&mut self, draft: &JournalEntryDraft) -> Result<String, KernelError>;

    /// Atomically links clearing postings and seals a settlement group, verifying that
    /// every linked clearing partition nets to exact zero.
    fn settle_group(
        &mut self,
        group_id: &str,
        name: &str,
        posting_ids: &[&str],
    ) -> Result<(), KernelError>;

    /// Reads the current signed normal balance for an account.
    fn get_account_balance(&self, account_id: &str) -> Result<i64, KernelError>;

    /// Computes and verifies the Fundamental Accounting Equation for (entity, currency).
    fn verify_accounting_equation(
        &self,
        entity_id: &str,
        currency: &str,
    ) -> Result<EquationSnapshot, KernelError>;
}
