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

## 3.1 The Kernel Invariant Guarantee & Bugcheck Policy

In Domain-Driven Design, **an aggregate never permits itself to enter an invalid state**.

Therefore, the accounting equation is **not** something the application ring must verify. The kernel guarantees it unconditionally across every committed mutation:

1. **Transactional Invariant Gate:** Every `post_entry` and `settle_group` call validates trial balance partitions in memory and inside SQLite triggers. If any partition fails to balance, the transaction aborts and rolls back to the prior valid state.

2. **Read-Side Intent (`summarize_accounting_equation`):** The interface method `summarize_accounting_equation` does not exist to ask the caller to double-check the kernel; it exists to provide the UI or reporting ring with a clean, aggregated snapshot of `Assets`, `Liabilities`, `Contributed Equity`, and `Current/Retained Earnings`.

3. **Fail-Fast Kernel Bugcheck:** If an internal state discrepancy ever arises, it represents a catastrophic kernel implementation defect, not a normal user validation error. In this event, the kernel halts the transaction and emits a **Kernel Bugcheck** containing:

   * The last known valid equation state.

   * The requested mutation that triggered the discrepancy.

   * Full diagnostic partition deltas.

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

// -----------------------------------------------------------------------------
// The Primary Domain Port: trait LedgerStore
// -----------------------------------------------------------------------------
// Any storage engine (SQLite, in-memory BTreeMap, PostgreSQL) can implement
// this trait without changing a single line of domain or recipe code.

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

    /// Reads a full journal entry aggregate back by ID.
    fn get_entry(&self, entry_id: &str) -> Result<Option<CommittedJournalEntry>, KernelError>;

    /// Lists active entries for an account with optional date range and pagination filters.
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

## 7. Application Usage Cookbook & Readback Reference

The following end-to-end patterns (implemented in `crates/accounting-cli/src/main.rs`) show how an application uses `LedgerStore` to write varied accounting events and read them back both individually and in aggregate.

For a workflow requiring multiple entries or application records, read `store.revision()`, then use `store.begin_transaction(expected_revision)`. The returned handle implements `LedgerStore`; `commit()` makes the staged writes durable together, while dropping it rolls them back. A settled group's entry cannot be replaced on its own; a coordinated correction operation is still pending.

### 7.1 Writing Varied Accounting Events

#### A. Multi-Currency Opening Balances (`USD` & `EUR`)
A single `JournalEntryDraft` can initialize accounts across multiple currencies as long as each `(entity_id, currency)` partition balances independently:

```rust
store.post_entry(&JournalEntryDraft::new(
    "2026-10-01",
    "Initial USD & EUR opening balances",
    vec![
        PostingPrimitive::new("hh_checking", "ent_household", Money::new(10000_00, "USD")?, PostingSide::Debit, "Opening checking"),
        PostingPrimitive::new("hh_savings", "ent_household", Money::new(5000_00, "USD")?, PostingSide::Debit, "Opening savings"),
        PostingPrimitive::new("hh_opening_eq", "ent_household", Money::new(15000_00, "USD")?, PostingSide::Credit, "Opening USD equity"),
        PostingPrimitive::new("hh_eur_wallet", "ent_household", Money::new(2000_00, "EUR")?, PostingSide::Debit, "Opening EUR wallet"),
        PostingPrimitive::new("hh_opening_eq_eur", "ent_household", Money::new(2000_00, "EUR")?, PostingSide::Credit, "Opening EUR equity"),
    ],
))?;
```

#### B. Purchase with Promotional Discount (`AccountRole::Contra`)
Because `Money` magnitudes are strictly positive (`amount_minor > 0`), a discount or instant rebate is recorded as a `Credit` to a `Contra` Expense account. Since `Expense` is Debit-normal, a `Credit` produces a negative normal balance (`-$150.00`), preserving both gross spend (`$1,200.00`) and net expense (`$1,050.00`):

```rust
let promo_entry_id = store.post_entry(&JournalEntryDraft::new(
    "2026-10-04",
    "Laptop purchase ($1,200 gross less $150 instant rebate)",
    vec![
        PostingPrimitive::new("hh_electronics", "ent_household", Money::new(1200_00, "USD")?, PostingSide::Debit, "Gross laptop price"),
        PostingPrimitive::new("hh_purchase_discounts", "ent_household", Money::new(150_00, "USD")?, PostingSide::Credit, "Instant promotional rebate"),
        PostingPrimitive::new("hh_visa", "ent_household", Money::new(1050_00, "USD")?, PostingSide::Credit, "Net Visa charge"),
    ],
))?;
```

#### C. 3-Way Cross-Entity Split Purchase (`Household`, `Business`, `Personal`)
When one entity pays a bill shared across multiple transparent entities, `RecipeCompiler::split_purchase_across_entities` generates balanced postings for every entity and pairs `Due From` (`Asset`) and `Due To` (`Liability`) positions:

```rust
let split_recipe = RecipeCompiler::split_purchase_across_entities(
    "2026-10-08",
    "ent_household",
    "hh_checking",
    AccountElement::Asset,
    "USD",
    &[
        SplitAllocation {
            target_entity_id: "ent_household",
            expense_account_id: "hh_insurance",
            amount_minor: 250_00,
            funding_due_from_account_id: None,
            target_due_to_account_id: None,
        },
        SplitAllocation {
            target_entity_id: "ent_business",
            expense_account_id: "biz_insurance",
            amount_minor: 250_00,
            funding_due_from_account_id: Some("hh_due_from_biz"),
            target_due_to_account_id: Some("biz_due_to_hh"),
        },
        SplitAllocation {
            target_entity_id: "ent_personal",
            expense_account_id: "per_insurance",
            amount_minor: 100_00,
            funding_due_from_account_id: Some("hh_due_from_per"),
            target_due_to_account_id: Some("per_due_to_hh"),
        },
    ],
    "Umbrella insurance policy ($600 split across Household, Business, Personal)",
)?;
let revision = store.revision()?;
let mut unit = store.begin_transaction(revision)?;
let split_entry_id = RecipeCompiler::post_recipe(&mut unit, &split_recipe)?;
unit.with_connection(|conn| RecipeCompiler::record_execution(conn, &split_recipe.execution, &split_entry_id))?;
unit.commit()?;
```

#### D. Multi-Date ACH Transfer & Payroll Clearing (`SettlementGroup`)
When legs of a transfer or payroll deposit settle on different dates, stage all required legs and seal their clearing group in one transaction. A missing leg rolls back the entire group:

```rust
// Leg 1 (Oct 10): Withdrawal from Checking into Transfer Clearing
let xfer_out_clear = PostingPrimitive::new("hh_xfer_clearing", "ent_household", Money::new(1000_00, "USD")?, PostingSide::Debit, "ACH transfer initiated");
let xfer_out_clear_id = xfer_out_clear.id.clone();
let revision = store.revision()?;
let mut unit = store.begin_transaction(revision)?;
unit.post_entry(&JournalEntryDraft::new(
    "2026-10-10",
    "ACH Transfer Out of Checking",
    vec![
        xfer_out_clear,
        PostingPrimitive::new("hh_checking", "ent_household", Money::new(1000_00, "USD")?, PostingSide::Credit, "Checking withdrawal"),
    ],
))?;

// Leg 2 (Oct 12): Deposit into Savings from Transfer Clearing
let xfer_in_clear = PostingPrimitive::new("hh_xfer_clearing", "ent_household", Money::new(1000_00, "USD")?, PostingSide::Credit, "ACH transfer settled");
let xfer_in_clear_id = xfer_in_clear.id.clone();
unit.post_entry(&JournalEntryDraft::new(
    "2026-10-12",
    "ACH Transfer Deposit into Savings",
    vec![
        PostingPrimitive::new("hh_savings", "ent_household", Money::new(1000_00, "USD")?, PostingSide::Debit, "Savings deposit"),
        xfer_in_clear,
    ],
))?;

// Seal group (enforces net 0 clearing residual for this specific group)
unit.settle_group("sg_ach_oct", "Oct 10-12 ACH Transfer ($1,000)", &[&xfer_out_clear_id, &xfer_in_clear_id])?;
unit.commit()?;
```

#### E. Fixing a Mistake (`JournalEntryDraft::with_replacement`)
When a user fixes a typo (e.g., `$950.00` instead of `$95.00`), `with_replacement` atomically transitions the old entry to `EntryStatus::Superseded` and seals the new entry as `EntryStatus::Active`:

```rust
let mistaken_id = store.post_entry(&JournalEntryDraft::new(
    "2026-10-18",
    "Weekly groceries (TYPO: entered $950.00 instead of $95.00)",
    vec![
        PostingPrimitive::new("hh_groceries", "ent_household", Money::new(950_00, "USD")?, PostingSide::Debit, "Typo amount"),
        PostingPrimitive::new("hh_checking", "ent_household", Money::new(950_00, "USD")?, PostingSide::Credit, "Checking debit"),
    ],
))?;

let corrected_id = store.post_entry(&JournalEntryDraft::with_replacement(
    "2026-10-18",
    "Weekly groceries (Corrected to $95.00)",
    &mistaken_id,
    vec![
        PostingPrimitive::new("hh_groceries", "ent_household", Money::new(95_00, "USD")?, PostingSide::Debit, "Corrected grocery receipt"),
        PostingPrimitive::new("hh_checking", "ent_household", Money::new(95_00, "USD")?, PostingSide::Credit, "Checking debit"),
    ],
))?;
```

---

### 7.2 Reading Back Specific Entries, Filtered Registers, & Aggregates

```rust
// 1. Read back a single entry with all postings
let entry = store.get_entry(&split_entry_id)?.expect("entry exists");

// 2. Query an account register for a date window (superseded entries are hidden automatically)
let filter = EntryFilter::date_range("2026-10-10", "2026-10-20");
let active_register = store.list_active_entries_for_account("hh_checking", &filter)?;

// 3. Reconstruct full forensic edit lineage on demand
let lineage = store.get_entry_lineage(&corrected_id)?;

// 4. Read aggregate Accounting Equation summary per (entity_id, currency)
let summary = store.summarize_accounting_equation("ent_household", "USD")?;
```

---

### 7.3 Verbatim Output (`cargo run -p accounting-cli`)

Running `cargo run -p accounting-cli` executes all 8 scenarios above and produces the following exact readback output:

```text
================================================================================
1. SPECIFIC ENTRY READBACK (`get_entry`)
================================================================================
  Entry [je_0000000a] | Date: 2026-10-04 (Eff: 2026-10-04) | Status: Active | Replaces: None
  Description: Laptop purchase ($1,200 gross less $150 instant rebate)
    - [DEBIT ] hh_electronics           (ent_household)   $1200.00 USD | Gross laptop price
    - [CREDIT] hh_purchase_discounts    (ent_household)    $150.00 USD | Instant promotional rebate
    - [CREDIT] hh_visa                  (ent_household)   $1050.00 USD | Net Visa charge

  Entry [je_00000016] | Date: 2026-10-08 (Eff: 2026-10-08) | Status: Active | Replaces: None
  Description: Umbrella insurance policy ($600 split across Household, Business, Personal)
    - [CREDIT] hh_checking              (ent_household)    $600.00 USD | Funding payment: Umbrella insurance policy ($600 split across Household, Business, Personal)
    - [DEBIT ] hh_insurance             (ent_household)    $250.00 USD | Direct expense: Umbrella insurance policy ($600 split across Household, Business, Personal)
    - [DEBIT ] hh_due_from_biz          (ent_household)    $250.00 USD | Inter-entity receivable from ent_business
    - [DEBIT ] biz_insurance            (ent_business )    $250.00 USD | Allocated expense paid by ent_household
    - [CREDIT] biz_due_to_hh            (ent_business )    $250.00 USD | Inter-entity payable to ent_household
    - [DEBIT ] hh_due_from_per          (ent_household)    $100.00 USD | Inter-entity receivable from ent_personal
    - [DEBIT ] per_insurance            (ent_personal )    $100.00 USD | Allocated expense paid by ent_household
    - [CREDIT] per_due_to_hh            (ent_personal )    $100.00 USD | Inter-entity payable to ent_household

================================================================================
2. DATE-FILTERED ACTIVE REGISTER (`hh_checking` from 2026-10-10 to 2026-10-20)
   Notice the $950 typo on 2026-10-18 is hidden; only the $95 fixup appears!
================================================================================
  Entry [je_0000001a] | Date: 2026-10-10 (Eff: 2026-10-10) | Status: Active | Replaces: None
  Description: ACH Transfer Out of Checking
    - [DEBIT ] hh_xfer_clearing         (ent_household)   $1000.00 USD | ACH transfer initiated
    - [CREDIT] hh_checking              (ent_household)   $1000.00 USD | Checking withdrawal
  ------------------------------------------------------------------------------
  Entry [je_00000025] | Date: 2026-10-15 (Eff: 2026-10-15) | Status: Active | Replaces: None
  Description: Oct 15 Payroll Direct Deposit
    - [DEBIT ] hh_checking              (ent_household)   $3200.00 USD | Direct deposit
    - [CREDIT] hh_payroll_clearing      (ent_household)   $3200.00 USD | Clear net pay
  ------------------------------------------------------------------------------
  Entry [je_0000002b] | Date: 2026-10-17 (Eff: 2026-10-17) | Status: Active | Replaces: None
  Description: Cover roommate share of utility deposit ($300)
    - [DEBIT ] hh_due_from_roommate     (ent_household)    $300.00 USD | Receivable from Roommate
    - [CREDIT] hh_checking              (ent_household)    $300.00 USD | Paid from checking
  ------------------------------------------------------------------------------
  Entry [je_00000037] | Date: 2026-10-18 (Eff: 2026-10-18) | Status: Active | Replaces: Some("je_00000034")
  Description: Weekly groceries (Corrected to $95.00)
    - [DEBIT ] hh_groceries             (ent_household)     $95.00 USD | Corrected grocery receipt
    - [CREDIT] hh_checking              (ent_household)     $95.00 USD | Checking debit
  ------------------------------------------------------------------------------
  Entry [je_0000002e] | Date: 2026-10-20 (Eff: 2026-10-20) | Status: Active | Replaces: None
  Description: Venmo partial repayment from roommate ($180)
    - [DEBIT ] hh_checking              (ent_household)    $180.00 USD | Venmo deposit
    - [CREDIT] hh_due_from_roommate     (ent_household)    $180.00 USD | Reduce roommate receivable
  ------------------------------------------------------------------------------

================================================================================
3. FORENSIC AUDIT LINEAGE (`get_entry_lineage` for corrected entry je_00000037)
================================================================================
  Revision #1:
  Entry [je_00000034] | Date: 2026-10-18 (Eff: 2026-10-18) | Status: Superseded | Replaces: None
  Description: Weekly groceries (TYPO: entered $950.00 instead of $95.00)
    - [DEBIT ] hh_groceries             (ent_household)    $950.00 USD | Typo amount
    - [CREDIT] hh_checking              (ent_household)    $950.00 USD | Checking debit
  Revision #2:
  Entry [je_00000037] | Date: 2026-10-18 (Eff: 2026-10-18) | Status: Active | Replaces: Some("je_00000034")
  Description: Weekly groceries (Corrected to $95.00)
    - [DEBIT ] hh_groceries             (ent_household)     $95.00 USD | Corrected grocery receipt
    - [CREDIT] hh_checking              (ent_household)     $95.00 USD | Checking debit

================================================================================
4. PER-ACCOUNT NORMAL BALANCES BY ENTITY
================================================================================
ENTITY          ACCOUNT_ID               ELEMENT    ROLE         NORMAL_BALANCE
ent_household   hh_due_from_biz          asset      standard        $250.00 USD
ent_household   hh_due_from_per          asset      standard        $100.00 USD
ent_household   hh_eur_wallet            asset      standard       €1955.00 EUR
ent_household   hh_checking              asset      standard      $11385.00 USD
ent_household   hh_hsa                   asset      standard        $250.00 USD
ent_household   hh_savings               asset      standard       $6000.00 USD
ent_household   hh_payroll_clearing      asset      clearing          $0.00 USD
ent_household   hh_xfer_clearing         asset      clearing          $0.00 USD
ent_household   hh_opening_eq_eur        equity     standard       €2000.00 EUR
ent_household   hh_opening_eq            equity     standard      $15000.00 USD
ent_household   hh_electronics           expense    standard       $1200.00 USD
ent_household   hh_eur_dining            expense    standard         €45.00 EUR
ent_household   hh_gifts_exp             expense    standard        $120.00 USD
ent_household   hh_groceries             expense    standard         $95.00 USD
ent_household   hh_insurance             expense    standard        $250.00 USD
ent_household   hh_tax_exp               expense    standard        $800.00 USD
ent_household   hh_purchase_discounts    expense    contra         -$150.00 USD
ent_household   hh_visa                  liability  standard       $1050.00 USD
ent_household   hh_hsa_match_rev         revenue    standard        $250.00 USD
ent_household   hh_salary_rev            revenue    standard       $4000.00 USD
ent_business    biz_insurance            expense    standard        $250.00 USD
ent_business    biz_due_to_hh            liability  standard        $250.00 USD
ent_personal    per_insurance            expense    standard        $100.00 USD
ent_personal    per_due_to_hh            liability  standard        $100.00 USD

================================================================================
5. AGGREGATE ACCOUNTING EQUATION SUMMARIES (`summarize_accounting_equation`)
   Assets == Liabilities + PaidInEquity + (Revenue - Expenses)
================================================================================
ENTITY          CURR        ASSETS  LIABILITIES  PAID_EQUITY      REVENUE     EXPENSES     TOTAL_EQ BALANCED
ent_household   USD       17985.00      1050.00     15000.00      4250.00      2315.00     16935.00     true
ent_household   EUR        1955.00         0.00      2000.00         0.00        45.00      1955.00     true
ent_business    USD           0.00       250.00         0.00         0.00       250.00      -250.00     true
ent_personal    USD           0.00       100.00         0.00         0.00       100.00      -100.00     true
```


---

## 8. Method-by-Method Design-by-Contract (DbC) Reference

An agent or developer using `LedgerStore` never needs to read the adapter implementation. Every method guarantees **all-or-nothing atomicity** and returns a deterministic, typed `KernelError` variant whenever a precondition is violated (`KernelError::Storage` is reserved strictly for underlying hardware/driver I/O faults).

| Method | Preconditions | Postconditions & Ordering | Exact `KernelError` Variants |
| :--- | :--- | :--- | :--- |
| **`register_entity`**`(entity_id, name, visibility)` | • `entity_id` is not already registered. | • Entity is persisted with `Transparent` or `Opaque` visibility. | • `DuplicateId { record_kind: "entity", id }` |
| **`create_ledger_account`**`(account_id, entity_id, name, currency, element, role)` | • `currency` is 3 uppercase ASCII letters.<br>• `account_id` is unique.<br>• `entity_id` exists and has `visibility == Transparent`.<br>• If `role == Clearing`, `element` is `Asset` or `Liability`. | • Account is created with `is_active = true` and `normal_balance = 0`. | • `InvalidCurrency(String)`<br>• `DuplicateId { record_kind: "ledger_account", id }`<br>• `UnknownEntity(String)`<br>• `AccountOwnerNotTransparent { entity_id }`<br>• `InvalidClearingElement { account_id, element }` |
| **`get_account`**`(account_id)` | • None. | • Returns `Ok(Some(LedgerAccount))` if found, or `Ok(None)` if not found. | • *(Only `Storage` on I/O failure)* |
| **`list_accounts`**`(entity_id)` | • None. | • Returns all accounts for `entity_id` ordered deterministically by `(element ASC, name ASC)`. Empty `Vec` if none exist. | • *(Only `Storage` on I/O failure)* |
| **`post_entry`**`(draft)` | • `draft.postings.len() >= 2`.<br>• Every `(entity_id, currency)` partition nets to `0` (`SUM(Dr) == SUM(Cr)`).<br>• `draft.id` is unique.<br>• Every posting's `ledger_account_id` exists and matches the account's `entity_id` and `currency`.<br>• If `draft.replaces_entry_id` is `Some(old_id)`, `old_id` exists, has `status == Active`, and is not in a settled group. | • If `replaces_entry_id` is set, `old_id` transitions atomically to `Superseded`.<br>• `draft` becomes `Active`; when called through a transaction handle it is durable only after `commit()`.<br>• Postings preserve their exact slice order. | • `InsufficientPostings(usize)`<br>• `UnbalancedPartitions(BTreeMap)`<br>• `ArithmeticOverflow`<br>• `DuplicateId { record_kind: "journal_entry", id }`<br>• `UnknownLedgerAccount(String)`<br>• `PostingAccountMismatch { .. }`<br>• `UnknownEntry(String)`<br>• `InvalidSupersedeTarget { entry_id, reason }` |
| **`get_entry`**`(entry_id)` | • None. | • Returns `Ok(Some(CommittedJournalEntry))` (regardless of `Active`, `Superseded`, or `Voided` status) with postings in insertion order, or `Ok(None)`. | • *(Only `Storage` on I/O failure)* |
| **`list_active_entries_for_account`**`(account_id, filter)` | • `filter.start_date` and `end_date` (if set) are ISO `YYYY-MM-DD` strings. | • Returns **only `EntryStatus::Active`** entries touching `account_id` within `[start_date, end_date]`, ordered by `(posting_date ASC, created_at ASC)`, sliced by `limit`/`offset`.<br>• Each returned entry includes **all** of its postings (not just the leg touching `account_id`). | • *(Only `Storage` on I/O failure)* |
| **`get_entry_lineage`**`(entry_id)` | • None. | • Walks backward via `replaces_entry_id` to the original root entry and forward to the latest replacement, returning the full chain `[Root(Superseded), ..., Latest(Active)]` in chronological revision order. Empty `Vec` if `entry_id` does not exist. | • *(Only `Storage` on I/O failure)* |
| **`settle_group`**`(group_id, name, posting_ids)` | • `posting_ids` is non-empty.<br>• `group_id` is unique.<br>• Every `posting_id` exists, belongs to an `Active` entry, and is not already in a settled group.<br>• For every `role == Clearing` account referenced in `posting_ids`, the sum of `Debits - Credits` within `posting_ids` is `0`. | • Group is persisted with `status = 'settled'` and all `posting_ids` are permanently locked to `group_id`. | • `EmptySettlementGroup { group_id }`<br>• `DuplicateId { record_kind: "settlement_group", id }`<br>• `UnknownPosting(String)`<br>• `SettlementPostingNotActive { .. }`<br>• `PostingAlreadySettled { posting_id }`<br>• `NonZeroClearingResidual { .. }` |
| **`get_account_balance`**`(account_id)` | • `account_id` exists. | • Returns the signed normal balance across all `Active` entries (`Dr - Cr` for `Asset`/`Expense`; `Cr - Dr` for `Liability`/`Equity`/`Revenue`). Returns `0` if no active postings exist. | • `UnknownLedgerAccount(String)` |
| **`summarize_accounting_equation`**`(entity_id, currency)` | • At least one `LedgerAccount` exists in `(entity_id, currency)`. | • Returns `AccountingEquationSummary` where `assets_minor == liabilities_minor + total_equity_minor` and `equation_discrepancy_minor == 0`. | • `UnknownEntityPartition { entity_id, currency }`<br>• `KernelBugcheck { .. }` *(if internal invariant ever breaks)* |

---

## 9. Agent & Developer Surprise Triage Protocol

When an agent or developer encounters unexpected behavior while using `LedgerStore`, use this 4-way decision tree to classify the root cause in one step without reading adapter internals:

```text
Unexpected Behavior Encountered
  │
  ├─► 1. Did the call return `Err(KernelError::KernelBugcheck { .. })`
  │      OR `Err(KernelError::Storage(_))` on valid in-memory inputs?
  │        └──► YES: [KERNEL IMPLEMENTATION BUG]
  │                  A domain precondition check or SQLite trigger/adapter
  │                  is out of sync. File a Kernel Bug with the returned error.
  │
  ├─► 2. Did the call return a typed `Err(KernelError::<Variant>)` listed in
  │      Section 8's Contract Table (e.g. `PostingAccountMismatch`,
  │      `NonZeroClearingResidual`, `AccountOwnerNotTransparent`)?
  │        └──► YES: [CALLER / APPLICATION RING BUG]
  │                  The application or recipe violated a documented accounting
  │                  precondition. Fix the caller/recipe inputs.
  │
  ├─► 3. Did the call succeed (`Ok(...)`), but the returned data/ordering/balance
  │      surprised the caller?
  │        ├─► Does the result match Section 8's Postconditions (e.g. `Contra`
  │        │   account having a negative normal balance, or `list_active_entries`
  │        │   hiding `Superseded` entries)?
  │        │     └──► YES: [WORKING AS DESIGNED] (See Section 2 & 8).
  │        └─► Was the behavior unspecified or ambiguous in Section 8?
  │              └──► YES: [KERNEL DOCUMENTATION BUG]
  │                        Update Section 8's Preconditions/Postconditions table
  │                        and add a contract test in `acceptance_suite.rs`.
  │
  └─► 4. Does the caller need a capability that `LedgerStore` does not offer?
           └──► Run the 4-Question Kernel Admission Litmus Test below.
```

### The 4-Question Kernel Admission Litmus Test (Kernel Feature vs. Application Ring)

Before adding any new table, column, or method to `accounting-kernel`, ask:

1. **Is it required to prove $\sum \text{Debits} = \sum \text{Credits}$, $\text{Assets} = \text{Liabilities} + \text{Equity}$, or zero-residual clearing?**
   * *If Yes* $\rightarrow$ **Core Kernel Feature Request.**
2. **Does it require atomic transaction coordination with `journal_entries` / `journal_postings` status transitions (e.g., voiding an active entry without a replacement, or multi-entry batch atomic commits)?**
   * *If Yes* $\rightarrow$ **Core Kernel Feature Request.** (For example, adding `void_entry(entry_id, reason)` to transition `Active -> Voided` is a genuine Core Kernel feature because `EntryStatus::Voided` is a kernel lifecycle state.)
3. **Can it be expressed by compiling business inputs into `JournalEntryDraft` and storing metadata in an outer table referencing `journal_entries(id)` or `ledger_accounts(id)`?**
   * *If Yes* $\rightarrow$ **Application Ring Feature** (belongs in `accounting-recipes`, `accounting-reconciliation`, or `accounting-reporting`).
4. **Does it mention external-world artifacts (bank statements, PDFs, OCR, tax forms, categories, tags, budgets, FX rate feeds, or MCP permissions)?**
   * *If Yes* $\rightarrow$ **Strictly Application Ring** (must never enter `accounting-kernel`).
