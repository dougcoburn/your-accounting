# Accounting Core Kernel — Interface Specification & Domain Guide

**Version:** 1.0.0-draft

**Architecture:** Domain-Driven Design (DDD) / Hexagonal Architecture (Ports & Adapters)

**Target:** Pure Double-Entry Accounting Core Engine

## 1. Architectural Charter & Boundary Rules

The **Accounting Core Kernel** is the mathematically pure, authoritative foundation of the entire accounting system. Its sole responsibility is to guarantee the integrity of double-entry bookkeeping across multi-entity, multi-currency books.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        APPLICATION & UX RING                           │
│  - Recipes (Business Intent Compilers: Split Purchases, Payroll)       │
│  - Sub-Ledger (Bank Accounts, Feeds, Observations, Movements)          │
│  - Statement Reconciliation (Assertions, Cutoff Windows)               │
│  - Classification (Hierarchical P&L Categories, Tags)                  │
│  - Evidence & Ingestion (PDFs, Invoices, OCR, Deduplication)           │
│  - Reporting & Analytics (P&L, Balance Sheet, Cash Flow)               │
│  - Security & External Agents (MCP Client Grants, Audit Operations)   │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                       calls domain interface (Port)
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                       DOMAIN KERNEL INTERFACE                          │
│  - Value Objects: Money, Currency, PostingSide, AccountElement         │
│  - Entities: LedgerAccount, AccountingEntity                           │
│  - Aggregates: JournalEntryDraft, PostingPrimitive, SettlementGroup    │
│  - Invariants: SUM(Dr) == SUM(Cr), Assets = Liabilities + Equity       │
│  - Repository Port: trait LedgerStore / trait LedgerRepository         │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                       implemented by adapter
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   INFRASTRUCTURE ADAPTER (STORAGE)                     │
│  - SQLite Transaction Boundary (In-Memory or File-Backed)              │
│  - Trigger-Level Defense-in-Depth (trg_journal_entry_seal_balance)     │
│  - (Optional Mock / Alternative Drivers for Testing)                   │
└────────────────────────────────────────────────────────────────────────┘

```

### The Inviolable Boundary Rule

1. **The Kernel MUST NOT know its storage engine:** The domain layer contains zero references to `rusqlite`, SQL strings, file paths, or database drivers. The persistence layer is an *adapter* implementing a domain *trait* (Port).

2. **The Kernel MUST NOT know how entries are compiled:** The kernel does not know about "recipes", "bank feeds", "rules", "splits", "tax rates", or "invoices". It only knows that a set of debit and credit postings was submitted and that they balance.

3. **The Application Ring MUST NOT inspect storage internals:** Outer layers interact exclusively through the high-level `LedgerStore` trait. They never receive a database connection or execute raw SQL against kernel tables.

## 2. Ubiquitous Language & Terminology

Every term in the Core Kernel has an exact mathematical and legal accounting definition:

### 2.1 Accounting Entities (`AccountingEntity`)

* **Transparent Entity (`transparent`):** An independent accounting entity that maintains its own self-balancing set of books (balance sheet and income statement). Examples: *Household*, *Consulting LLC*, *Personal*.

* **Opaque Entity (`opaque`):** An external counterparty (person or organization) tracked for receivables, payables, or obligations, but whose internal books are not maintained by this system. Examples: *Employer*, *Roommate*, *Child*.

### 2.2 Currency & Monetary Values (`Currency`, `Money`)

* **Currency:** An immutable 3-letter uppercase ASCII ISO-4217 code (e.g. `USD`, `EUR`, `CAD`, `JPY`). The kernel treats different currencies as distinct mathematical dimensions; there is **no implicit cross-currency summing**.

* **Money:** An exact integer minor-unit magnitude (\$1.00 USD = 100 minor units; 1 JPY = 1 minor unit) coupled with its `Currency`. **IEEE floating-point numbers are strictly forbidden.** All operations use checked integer arithmetic.

### 2.3 The 5 Fundamental Account Elements (`AccountElement`)

Every ledger account belongs to exactly one of the five universal accounting elements:

$$
\text{Assets} + \text{Expenses} = \text{Liabilities} + \text{Equity} + \text{Revenue}
$$

| Element | Normal Balance | Balance Formula | Economic Role | 
| ----- | ----- | ----- | ----- | 
| **`Asset`** | **Debit** | $\sum \text{Dr} - \sum \text{Cr}$ | Economic resources owned (Cash, Checking, Investments, Receivables). | 
| **`Liability`** | **Credit** | $\sum \text{Cr} - \sum \text{Dr}$ | Debts and obligations owed to outsiders (Credit cards, Loans, Payables). | 
| **`Equity`** | **Credit** | $\sum \text{Cr} - \sum \text{Dr}$ | Residual interest / capital (Contributed capital, Retained earnings). | 
| **`Revenue`** | **Credit** | $\sum \text{Cr} - \sum \text{Dr}$ | Gross inflows from earnings (Salary, Sales, Dividends, Interest). | 
| **`Expense`** | **Debit** | $\sum \text{Dr} - \sum \text{Cr}$ | Outflows incurred to generate revenue or living costs (Food, Rent). | 

* **Normal Balance:** The side (`Debit` or `Credit`) where an increase in the account is recorded. An entry on the normal side increases the account's balance; an entry on the opposite side decreases it.

### 2.4 Ledger Accounts (`LedgerAccount`)

An engine-managed accumulator of debits and credits. Every ledger account has:

1. `id`: Stable identifier.

2. `entity_id`: The single transparent entity that owns this account.

3. `currency`: The single ISO-4217 currency in which all postings to this account must be denominated.

4. `element`: Exactly one of the 5 elements (`Asset`, `Liability`, `Equity`, `Revenue`, `Expense`).

5. `role`: Sub-classification for kernel engine mechanics:

   * `Standard`: Ordinary balance-sheet or P&L account.

   * `Clearing`: Temporary transit account used across multi-step events (e.g. bank transfer transit, payroll clearing). Must net to zero.

   * `InterEntity`: Balance-sheet due-to/due-from account representing obligations between two transparent entities.

   * `Contra`: Reduces the balance of a companion account within the same element.

### 2.5 Journal Postings (`PostingPrimitive`)

The atomic unit of double entry. A single debit or credit line:

* `ledger_account_id`: Target account.

* `entity_id`: Entity owning the account (must match account).

* `money`: Magnitude (`amount_minor > 0`) and `Currency` (must match account).

* `side`: `PostingSide::Debit` or `PostingSide::Credit`.

* `memo`: Human-readable annotation.

### 2.6 Journal Entries (`JournalEntryDraft` / `JournalEntry`)

An atomic financial transaction composed of two or more `PostingPrimitive` lines.

* **`draft`:** An in-flight proposal being assembled in memory. Has zero legal or accounting effect.

* **`posted`:** An immutable, committed transaction that has passed all domain invariants. Once posted, lines can never be updated, inserted, or deleted.

### 2.7 Settlement Groups (`SettlementGroup`)

A flat reconciliation boundary that aggregates multiple clearing postings across independent events (e.g., Leg 1 of a bank transfer on Monday and Leg 2 on Wednesday, or Gross Payroll plus Net Deposit plus HSA Deposit). A settlement group cannot be marked `settled` unless the net residual of all linked clearing postings is exactly **zero** per `(entity_id, currency, ledger_account_id)`.

## 3. Mathematical Invariants (Machine-Enforced)

The Core Kernel guarantees four mathematical invariants. If any invariant is violated, the operation is aborted and no state changes:

### Invariant 1: Trial Balance Invariant (Per Partition)

For every committed `JournalEntry`, the sum of debits must equal the sum of credits within every `(entity_id, currency)` partition:

$$
\forall (e, c) \in \text{Partitions}(\text{Entry}): \quad \sum_{p \in \text{Debits}(e, c)} \text{Amount}(p) = \sum_{p \in \text{Credits}(e, c)} \text{Amount}(p)
$$

### Invariant 2: The Fundamental Accounting Equation

Across all committed entries in the ledger, the accounting equation holds at all times for every `(entity_id, currency)`:

$$
\text{Assets} = \text{Liabilities} + \text{Equity} + (\text{Revenue} - \text{Expenses})
$$

$$
\text{Total Equity} \equiv \text{Contributed Equity} + \underbrace{(\text{Revenue} - \text{Expenses})}_{\text{Current / Retained Earnings}}
$$

$$
\text{Equation Discrepancy} \equiv \text{Assets} - \big(\text{Liabilities} + \text{Total Equity}\big) = 0
$$

### Invariant 3: Post-Commit Immutability

Once a `JournalEntry` transitions to `status = 'posted'`, it is permanently sealed:

* No postings may be inserted, edited, or removed.

* The entry cannot be deleted.

* Any correction must be accomplished by posting a **new** reversing or adjusting `JournalEntry` linked via `replaces_entry_id`.

### Invariant 4: Zero-Residual Settlement Group Partitioning

A `SettlementGroup` can only be sealed (`status = 'settled'`) if every linked `role = 'clearing'` account has a net balance of zero:

$$
\forall (e, c, a) \in \text{ClearingAccounts}(\text{Group}): \quad \sum_{p \in \text{Debits}(e, c, a)} \text{Amount}(p) - \sum_{p \in \text{Credits}(e, c, a)} \text{Amount}(p) = 0
$$

Errors or residuals in one settlement group cannot offset or cross-cancel errors in another.

## 4. Kernel vs. Application Ring Boundary Audit

To prevent future architecture erosion, this table details what belongs in the **Kernel** versus what belongs in the **Application Ring**:

| Concept | Layer | Why It Belongs Here | 
| ----- | ----- | ----- | 
| **Double-Entry Arithmetic** | **Core Kernel** | Mathematical truth ($\sum \text{Dr} = \sum \text{Cr}$). Agnostic to all business contexts. | 
| **5 Account Elements** | **Core Kernel** | Structural definition of balance-sheet and P&L normal balance signs. | 
| **Zero-Residual Clearing** | **Core Kernel** | Mathematical guarantee that transit accounts net to zero before group settlement. | 
| **Recipes & Compilers** | **Application Ring** | Business intent ("Split Costco bill 66/33"). Produces journal entry drafts. | 
| **Recipe Provenance Tables** | **Application Ring** | Keeps input parameters (`parameters_json`), split ratios, and user options outside the immutable ledger. | 
| **Bank Feeds & Movements** | **Application Ring** | Real-world statements contain pending items, duplicates, and unverified data that cannot enter the double-entry ledger until validated. | 
| **Statement Assertions** | **Application Ring** | External cutoff arithmetic comparing bank balance statements against ledger movements. | 
| **Category Trees & Tags** | **Application Ring** | User-facing classification taxonomy. Leaf categories map to kernel Revenue/Expense accounts or act as metadata overlays. | 
| **Evidence & OCR** | **Application Ring** | PDF blobs, SHA-256 hashes, and extracted candidate text. | 
| **MCP Authorization** | **Application Ring** | Scoped agent permissions, tools, and security barriers. | 

## 5. The Pure Rust Kernel Interface (Domain Port)

The domain interface is defined as pure Rust traits and structs. It has **no dependency** on `rusqlite` or any concrete database.

```
use std::collections::BTreeMap;
use std::fmt;

// -----------------------------------------------------------------------------
// Pure Domain Value Objects & Primitives
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PostingSide {
    Debit,
    Credit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountElement {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountRole {
    Standard,
    Clearing,
    InterEntity,
    Contra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityVisibility {
    Transparent,
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    amount_minor: i64,
    currency: Currency,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntryDraft {
    pub id: String,
    pub posting_date: String,
    pub effective_date: Option<String>,
    pub description: String,
    pub replaces_entry_id: Option<String>,
    pub postings: Vec<PostingPrimitive>,
}

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

// -----------------------------------------------------------------------------
// The Primary Domain Port: trait LedgerStore
// -----------------------------------------------------------------------------
// Any storage engine (SQLite, in-memory BTreeMap, PostgreSQL) can implement
// this trait without changing a single line of domain or recipe code.

pub trait LedgerStore {
    /// Registers a transparent or opaque accounting entity.
    fn register_entity(
        &mut self,
        entity_id: &str,
        name: &str,
        visibility: EntityVisibility,
    ) -> Result<(), KernelError>;

    /// Creates an engine-managed ledger account.
    fn create_ledger_account(
        &mut self,
        account_id: &str,
        entity_id: &str,
        name: &str,
        currency: &str,
        element: AccountElement,
        role: AccountRole,
    ) -> Result<(), KernelError>;

    /// Atomically validates and commits a balanced JournalEntryDraft to the immutable ledger.
    /// Returns the committed entry ID.
    fn post_entry(&mut self, draft: &JournalEntryDraft) -> Result<String, KernelError>;

    /// Atomically links clearing postings and seals a settlement group, verifying that
    /// the net residual of all clearing partitions in the group is exactly zero.
    fn settle_group(
        &mut self,
        group_id: &str,
        name: &str,
        posting_ids: &[&str],
    ) -> Result<(), KernelError>;

    /// Retrieves the current normal balance of a ledger account.
    fn get_account_balance(&self, account_id: &str) -> Result<i64, KernelError>;

    /// Computes and verifies the Fundamental Accounting Equation for an entity and currency.
    fn verify_accounting_equation(
        &self,
        entity_id: &str,
        currency: &str,
    ) -> Result<EquationSnapshot, KernelError>;
}

```

## 6. How the Application Ring Consumes the Kernel

The application ring (e.g. `accounting-recipes`) interacts with the kernel **only** via the `LedgerStore` trait:

```
pub struct RecipeCompiler;

impl RecipeCompiler {
    /// Compiles high-level business split into a balanced JournalEntryDraft
    /// and an accompanying RecipeExecution metadata record.
    pub fn split_purchase_across_entities(
        posting_date: &str,
        funding_entity_id: &str,
        funding_account_id: &str,
        currency: &str,
        allocations: &[SplitAllocation<'_>],
        description: &str,
    ) -> Result<RecipeOutput, KernelError> {
        // 1. Compute exact mathematics & inter-entity obligations
        // 2. Return balanced JournalEntryDraft + RecipeExecution
    }

    /// Posts via the abstract LedgerStore trait
    pub fn post_recipe<S: LedgerStore>(
        store: &mut S,
        output: &RecipeOutput,
    ) -> Result<String, KernelError> {
        // Core ledger commit
        let entry_id = store.post_entry(&output.draft)?;
        
        // (Optional: record output.execution in the recipe extension store)
        Ok(entry_id)
    }
}

```

## 7. Implementation Roadmap & Checklist

* \[x\] **Core Accounting Invariants Defined:** `SUM(Dr) == SUM(Cr)`, `Assets = Liabilities + Equity`, zero-residual clearing.

* \[x\] **Decoupled Architecture:** `recipe_id` removed from kernel; moved to `recipe_executions` in the application ring.

* \[x\] **Pure Trait Port (`LedgerStore`):** Kernel domain defined without direct SQLite type leaks.

* \[ \] **Infrastructure Separation:** Move `rusqlite` into a separate adapter submodule or crate (`accounting-storage-sqlite`) so `accounting-kernel` compiles with zero database dependencies.

* \[ \] **Sub-Ledger Port (`crates/accounting-reconciliation`):** Bank movements, statements, and balance assertions that compile into verified entries.