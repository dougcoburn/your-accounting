//! Typed account and party values, plus the injected clock and id source.
//!
//! There is no visibility flag and no contra type (ADR 0003, ADR 0008).

use crate::v2::error::{Code, Error};

/// Injected clock. `begin_transaction` reads it once.
pub trait Clock {
    fn now_utc(&mut self) -> String;
}

/// Injected ULID source. Caller-supplied ids are kept when they pass the check.
pub trait IdSource {
    fn next_id(&mut self) -> String;
}

/// Clock that always returns the same timestamp.
pub struct FixedClock {
    pub timestamp: String,
}

impl Clock for FixedClock {
    fn now_utc(&mut self) -> String {
        self.timestamp.clone()
    }
}

/// Ids `01ARZ3NDEKTSV4RRFFQ69G` plus a 4-character Crockford counter.
pub struct SequenceIds {
    next: u32,
}

impl SequenceIds {
    pub fn new() -> Self {
        Self { next: 0 }
    }
}

impl Default for SequenceIds {
    fn default() -> Self {
        Self::new()
    }
}

impl IdSource for SequenceIds {
    fn next_id(&mut self) -> String {
        self.next = self.next.saturating_add(1);
        format!("01ARZ3NDEKTSV4RRFFQ69G{}", crockford4(self.next))
    }
}

fn crockford4(n: u32) -> String {
    const ALPH: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut chars = [b'0'; 4];
    let mut x = n;
    for slot in chars.iter_mut().rev() {
        *slot = ALPH[(x & 31) as usize];
        x >>= 5;
    }
    String::from_utf8(chars.to_vec()).unwrap_or_else(|_| "0000".to_string())
}

/// 26-character Crockford base32. `I`, `L`, `O`, and `U` are rejected.
pub fn is_ulid(value: &str) -> bool {
    if value.len() != 26 {
        return false;
    }
    value.bytes().all(|b| {
        matches!(b,
            b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'
        )
    })
}

pub fn require_ulid(value: &str) -> Result<(), Error> {
    if is_ulid(value) {
        Ok(())
    } else {
        Err(Error::new(Code::InvalidId, &[], vec![]))
    }
}

pub fn require_currency(value: &str) -> Result<(), Error> {
    let bytes = value.as_bytes();
    if bytes.len() == 3 && bytes.iter().all(|b| b.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(Error::new(Code::InvalidCurrency, &[], vec![]))
    }
}

pub fn require_name(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.contains('\n') {
        Err(Error::new(Code::InvalidShape, &[], vec![]))
    } else {
        Ok(())
    }
}

pub fn require_positive(amount: i64) -> Result<(), Error> {
    if amount > 0 {
        Ok(())
    } else {
        Err(Error::new(Code::InvalidMoney, &[], vec![]))
    }
}

/// Canonical decimal for the inbox payload hash: no sign, no leading zeros.
pub fn canonical_amount(amount: i64) -> Result<String, Error> {
    require_positive(amount)?;
    Ok(amount.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountType {
    Asset,
    Liability,
    Equity,
    Income,
    Expense,
}

impl AccountType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Liability => "liability",
            Self::Equity => "equity",
            Self::Income => "income",
            Self::Expense => "expense",
        }
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "asset" => Ok(Self::Asset),
            "liability" => Ok(Self::Liability),
            "equity" => Ok(Self::Equity),
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            _ => Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("account_type")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerKind {
    Cash,
    Card,
    Loan,
    Investment,
    FixedAsset,
    Receivable,
    Payable,
    Clearing,
    OpeningEquity,
    Equity,
    Category,
}

impl LedgerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::Card => "card",
            Self::Loan => "loan",
            Self::Investment => "investment",
            Self::FixedAsset => "fixed_asset",
            Self::Receivable => "receivable",
            Self::Payable => "payable",
            Self::Clearing => "clearing",
            Self::OpeningEquity => "opening_equity",
            Self::Equity => "equity",
            Self::Category => "category",
        }
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "cash" => Ok(Self::Cash),
            "card" => Ok(Self::Card),
            "loan" => Ok(Self::Loan),
            "investment" => Ok(Self::Investment),
            "fixed_asset" => Ok(Self::FixedAsset),
            "receivable" => Ok(Self::Receivable),
            "payable" => Ok(Self::Payable),
            "clearing" => Ok(Self::Clearing),
            "opening_equity" => Ok(Self::OpeningEquity),
            "equity" => Ok(Self::Equity),
            "category" => Ok(Self::Category),
            _ => Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("ledger_kind")),
        }
    }

    pub fn fits(self, account_type: AccountType) -> bool {
        let category = matches!(account_type, AccountType::Income | AccountType::Expense);
        match self {
            Self::Category => category,
            Self::Cash | Self::Investment | Self::FixedAsset | Self::Receivable => {
                account_type == AccountType::Asset
            }
            Self::Card | Self::Loan | Self::Payable => account_type == AccountType::Liability,
            Self::OpeningEquity | Self::Equity => account_type == AccountType::Equity,
            Self::Clearing => matches!(account_type, AccountType::Asset | AccountType::Liability),
        }
    }

    pub fn allows_financial_account(self) -> bool {
        matches!(
            self,
            Self::Cash | Self::Card | Self::Loan | Self::Investment | Self::FixedAsset
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinancialKind {
    Checking,
    Savings,
    Cash,
    GiftCard,
    StoreCredit,
    CreditCard,
    Loan,
    Brokerage,
    Retirement,
    Hsa,
    RealEstate,
    Vehicle,
    OtherAsset,
    OtherLiability,
}

impl FinancialKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Savings => "savings",
            Self::Cash => "cash",
            Self::GiftCard => "gift_card",
            Self::StoreCredit => "store_credit",
            Self::CreditCard => "credit_card",
            Self::Loan => "loan",
            Self::Brokerage => "brokerage",
            Self::Retirement => "retirement",
            Self::Hsa => "hsa",
            Self::RealEstate => "real_estate",
            Self::Vehicle => "vehicle",
            Self::OtherAsset => "other_asset",
            Self::OtherLiability => "other_liability",
        }
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "checking" => Ok(Self::Checking),
            "savings" => Ok(Self::Savings),
            "cash" => Ok(Self::Cash),
            "gift_card" => Ok(Self::GiftCard),
            "store_credit" => Ok(Self::StoreCredit),
            "credit_card" => Ok(Self::CreditCard),
            "loan" => Ok(Self::Loan),
            "brokerage" => Ok(Self::Brokerage),
            "retirement" => Ok(Self::Retirement),
            "hsa" => Ok(Self::Hsa),
            "real_estate" => Ok(Self::RealEstate),
            "vehicle" => Ok(Self::Vehicle),
            "other_asset" => Ok(Self::OtherAsset),
            "other_liability" => Ok(Self::OtherLiability),
            _ => Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("financial_kind")),
        }
    }
}

pub fn ledger_matches_financial(ledger: LedgerKind, financial: FinancialKind) -> bool {
    match ledger {
        LedgerKind::Cash => matches!(
            financial,
            FinancialKind::Checking
                | FinancialKind::Savings
                | FinancialKind::Cash
                | FinancialKind::GiftCard
                | FinancialKind::StoreCredit
                | FinancialKind::OtherAsset
        ),
        LedgerKind::Card => financial == FinancialKind::CreditCard,
        LedgerKind::Loan => matches!(
            financial,
            FinancialKind::Loan | FinancialKind::OtherLiability
        ),
        LedgerKind::Investment => matches!(
            financial,
            FinancialKind::Brokerage | FinancialKind::Retirement | FinancialKind::Hsa
        ),
        LedgerKind::FixedAsset => matches!(
            financial,
            FinancialKind::RealEstate | FinancialKind::Vehicle | FinancialKind::OtherAsset
        ),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingMode {
    Transparent,
    BlackBox,
}

impl TrackingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transparent => "transparent",
            Self::BlackBox => "black_box",
        }
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "transparent" => Ok(Self::Transparent),
            "black_box" => Ok(Self::BlackBox),
            _ => Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("tracking_mode")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyLabel {
    Party,
    Vendor,
    Person,
    Organization,
    Employer,
}

impl PartyLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Party => "party",
            Self::Vendor => "vendor",
            Self::Person => "person",
            Self::Organization => "organization",
            Self::Employer => "employer",
        }
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "party" => Ok(Self::Party),
            "vendor" => Ok(Self::Vendor),
            "person" => Ok(Self::Person),
            "organization" => Ok(Self::Organization),
            "employer" => Ok(Self::Employer),
            _ => Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("party_label")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Debit,
    Credit,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "debit" => Ok(Self::Debit),
            "credit" => Ok(Self::Credit),
            _ => Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("posting_side")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxAction {
    Leg,
    PartialSettlement,
    Forgiveness,
    Correction,
}

impl InboxAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Leg => "leg",
            Self::PartialSettlement => "partial_settlement",
            Self::Forgiveness => "forgiveness",
            Self::Correction => "correction",
        }
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "leg" => Ok(Self::Leg),
            "partial_settlement" => Ok(Self::PartialSettlement),
            "forgiveness" => Ok(Self::Forgiveness),
            "correction" => Ok(Self::Correction),
            _ => Err(Error::new(Code::InvalidShape, &[], vec![]).with_token("outbox_action")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Entity,
    Party,
    FinancialAccount,
    LedgerAccount,
    Tag,
    Feed,
}

impl ReferenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Party => "party",
            Self::FinancialAccount => "financial_account",
            Self::LedgerAccount => "ledger_account",
            Self::Tag => "tag",
            Self::Feed => "feed",
        }
    }
}

pub fn shape(token: &'static str) -> Error {
    Error::new(Code::InvalidShape, &[], vec![]).with_token(token)
}
