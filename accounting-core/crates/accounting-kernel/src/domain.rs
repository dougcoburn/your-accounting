//! Pure Domain Value Objects, Aggregate Roots, and Port Traits for Double-Entry Bookkeeping.
//!
//! Contains zero storage dependencies. Persistence is completely abstracted behind `LedgerStore`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_deterministic_id(prefix: &str) -> String {
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{seq:08x}")
}

// =============================================================================
// DOMAIN ERRORS
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
    UnknownEntry(String),
    InvalidSupersedeTarget { entry_id: String, reason: String },
    EquationViolation(AccountingEquationSummary),
    Storage(String),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCurrency(c) => write!(f, "Currency must be a 3-letter uppercase ISO code, got {c:?}"),
            Self::NonPositiveAmount(a) => write!(f, "Posting Money magnitude must be > 0, got {a}"),
            Self::ArithmeticOverflow => write!(f, "Checked integer arithmetic overflow"),
            Self::InsufficientPostings(n) => write!(f, "INV-001: A JournalEntry must contain at least 2 postings (got {n})"),
            Self::UnbalancedPartitions(map) => write!(f, "INV-001: Unbalanced (entity, currency) partitions: {map:?}"),
            Self::MissingInterEntityAccounts { target_entity_id } => write!(f, "Cross-entity split for {target_entity_id} requires paired due_from and due_to accounts"),
            Self::UnknownLedgerAccount(id) => write!(f, "Unknown ledger_account_id: {id}"),
            Self::UnknownEntityPartition { entity_id, currency } => write!(f, "No ledger accounts found for ({entity_id}, {currency})"),
            Self::UnknownEntry(id) => write!(f, "Unknown journal_entry_id: {id}"),
            Self::InvalidSupersedeTarget { entry_id, reason } => write!(f, "Cannot supersede entry {entry_id}: {reason}"),
            Self::EquationViolation(snap) => write!(f, "Accounting equation violated: {snap:?}"),
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

    pub fn parse(s: &str) -> Result<Self, KernelError> {
        match s {
            "debit" => Ok(Self::Debit),
            "credit" => Ok(Self::Credit),
            _ => Err(KernelError::Storage(format!("invalid posting side: {s}"))),
        }
    }
}

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

    pub fn parse(s: &str) -> Result<Self, KernelError> {
        match s {
            "asset" => Ok(Self::Asset),
            "liability" => Ok(Self::Liability),
            "equity" => Ok(Self::Equity),
            "revenue" => Ok(Self::Revenue),
            "expense" => Ok(Self::Expense),
            _ => Err(KernelError::Storage(format!("invalid account element: {s}"))),
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

    pub fn parse(s: &str) -> Result<Self, KernelError> {
        match s {
            "standard" => Ok(Self::Standard),
            "clearing" => Ok(Self::Clearing),
            "contra" => Ok(Self::Contra),
            _ => Err(KernelError::Storage(format!("invalid account role: {s}"))),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryStatus {
    Draft,
    Active,
    Superseded,
    Voided,
}

impl EntryStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Voided => "voided",
        }
    }

    pub fn parse(s: &str) -> Result<Self, KernelError> {
        match s {
            "draft" => Ok(Self::Draft),
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "voided" => Ok(Self::Voided),
            _ => Err(KernelError::Storage(format!("invalid entry status: {s}"))),
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

// =============================================================================
// 2. READ & WRITE AGGREGATES
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerAccount {
    pub id: String,
    pub entity_id: String,
    pub name: String,
    pub currency: Currency,
    pub element: AccountElement,
    pub role: AccountRole,
    pub is_active: bool,
}

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

    pub fn with_replacement(
        posting_date: impl Into<String>,
        description: impl Into<String>,
        replaces_entry_id: impl Into<String>,
        postings: Vec<PostingPrimitive>,
    ) -> Self {
        Self {
            id: next_deterministic_id("je"),
            posting_date: posting_date.into(),
            effective_date: None,
            description: description.into(),
            replaces_entry_id: Some(replaces_entry_id.into()),
            postings,
        }
    }

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

/// Symmetrical committed journal entry read back from storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedJournalEntry {
    pub id: String,
    pub posting_date: String,
    pub effective_date: String,
    pub description: String,
    pub status: EntryStatus,
    pub replaces_entry_id: Option<String>,
    pub created_at: String,
    pub postings: Vec<PostingPrimitive>,
}

/// Query filter for searching entries for an account.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryFilter {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl EntryFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn date_range(start_date: impl Into<String>, end_date: impl Into<String>) -> Self {
        Self {
            start_date: Some(start_date.into()),
            end_date: Some(end_date.into()),
            limit: None,
            offset: None,
        }
    }

    pub fn with_pagination(mut self, limit: usize, offset: usize) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Read-model verification of `Assets == Liabilities + Equity + (Revenue - Expenses)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountingEquationSummary {
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

impl AccountingEquationSummary {
    pub const fn is_balanced(&self) -> bool {
        self.trial_debits_minor == self.trial_credits_minor
            && self.assets_minor == (self.liabilities_minor + self.total_equity_minor)
            && self.equation_discrepancy_minor == 0
    }
}

// =============================================================================
// 3. THE COMPLETE PRIMARY DOMAIN PORT (TRAIT LEDGERSTORE)
// =============================================================================

pub trait LedgerStore {
    // --- Entity & Account Management ---
    fn register_entity(
        &mut self,
        entity_id: &str,
        name: &str,
        visibility: EntityVisibility,
    ) -> Result<(), KernelError>;

    fn create_ledger_account(
        &mut self,
        account_id: &str,
        entity_id: &str,
        name: &str,
        currency: &str,
        element: AccountElement,
        role: AccountRole,
    ) -> Result<(), KernelError>;

    fn get_account(&self, account_id: &str) -> Result<Option<LedgerAccount>, KernelError>;
    fn list_accounts(&self, entity_id: &str) -> Result<Vec<LedgerAccount>, KernelError>;

    // --- Journal Posting & Fixups ---
    /// Atomically validates in-memory, posts, and seals a JournalEntryDraft.
    /// If draft.replaces_entry_id is set, atomically transitions the target entry to `superseded`.
    fn post_entry(&mut self, draft: &JournalEntryDraft) -> Result<String, KernelError>;

    fn get_entry(&self, entry_id: &str) -> Result<Option<CommittedJournalEntry>, KernelError>;

    /// Lists active (non-superseded, non-voided) journal entries touching an account within an optional date range.
    fn list_active_entries_for_account(
        &self,
        account_id: &str,
        filter: &EntryFilter,
    ) -> Result<Vec<CommittedJournalEntry>, KernelError>;

    /// Retrieves the complete edit history / lineage for an entry (old mistakes + fixups in chronological order).
    fn get_entry_lineage(&self, entry_id: &str) -> Result<Vec<CommittedJournalEntry>, KernelError>;

    // --- Settlement & Clearing ---
    fn settle_group(
        &mut self,
        group_id: &str,
        name: &str,
        posting_ids: &[&str],
    ) -> Result<(), KernelError>;

    // --- Balances & Mathematical Invariants ---
    fn get_account_balance(&self, account_id: &str) -> Result<i64, KernelError>;

    fn summarize_accounting_equation(
        &self,
        entity_id: &str,
        currency: &str,
    ) -> Result<AccountingEquationSummary, KernelError>;
}
