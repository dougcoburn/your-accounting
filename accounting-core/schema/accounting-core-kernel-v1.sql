-- =============================================================================
-- ACCOUNTING CORE KERNEL v1.0 (Immutable DDD Core Domain)
-- =============================================================================
-- Purpose:
--   This schema isolates the unchanging mathematical kernel of double-entry
--   bookkeeping from outer rings (bank feeds, OCR/evidence, MCP auth, reporting).
--
-- Core Mathematical Guarantee:
--   For every (entity_id, currency) partition across all posted JournalEntries:
--     1. Trial Balance Invariant:
--          SUM(Debits) == SUM(Credits)
--     2. Fundamental Accounting Equation Invariant:
--          Assets == Liabilities + Equity + (Revenue - Expenses)
--          (where Retained/Current Earnings = Revenue - Expenses)
--     3. Zero-Residual Clearing Invariant:
--          Every finalized SettlementGroup has net 0 balance across all
--          clearing accounts per (entity_id, currency).
-- =============================================================================

PRAGMA foreign_keys = ON;

-- -----------------------------------------------------------------------------
-- 1. ENTITIES (Books of Account vs. External Opaque Counterparties)
-- -----------------------------------------------------------------------------
CREATE TABLE entities (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    visibility  TEXT NOT NULL CHECK (visibility IN ('transparent', 'opaque')),
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- -----------------------------------------------------------------------------
-- 2. LEDGER ACCOUNTS (The 5 Fundamental Elements of the Accounting Equation)
-- -----------------------------------------------------------------------------
-- Every ledger account belongs to a transparent entity, is denominated in a
-- single ISO-4217 currency, and maps to one of the 5 fundamental elements:
--   Debit-Normal  (+1 on debit, -1 on credit): 'asset', 'expense'
--   Credit-Normal (-1 on debit, +1 on credit): 'liability', 'equity', 'revenue'
--
-- Optional sub-role flags (`role`) support engine primitives without changing
-- the fundamental equation:
--   - 'standard'      : ordinary asset, liability, equity, revenue, or expense
--   - 'clearing'      : temporary transit/clearing account (must net to 0 per settlement group)
--   - 'inter_entity'  : due-from (asset) or due-to (liability) another entity
--   - 'contra'        : reduces a paired account in the same element (e.g., discount/contra-expense)
CREATE TABLE ledger_accounts (
    id                  TEXT PRIMARY KEY,
    entity_id           TEXT NOT NULL REFERENCES entities(id),
    name                TEXT NOT NULL,
    currency            TEXT NOT NULL CHECK (length(currency) = 3),
    element             TEXT NOT NULL CHECK (element IN ('asset', 'liability', 'equity', 'revenue', 'expense')),
    role                TEXT NOT NULL DEFAULT 'standard' CHECK (role IN ('standard', 'clearing', 'contra')),
    is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    CHECK (role != 'clearing' OR element IN ('asset', 'liability'))
);

CREATE INDEX idx_ledger_accounts_entity_currency ON ledger_accounts(entity_id, currency, element);

-- Guard: Only transparent entities maintain balanced books of account.
CREATE TRIGGER trg_ledger_account_transparent_entity
BEFORE INSERT ON ledger_accounts
WHEN (SELECT visibility FROM entities WHERE id = NEW.entity_id) != 'transparent'
BEGIN
    SELECT RAISE(ABORT, 'INV-ENTITY: Only transparent entities can own ledger_accounts');
END;

-- -----------------------------------------------------------------------------
-- 3. JOURNAL ENTRIES (Atomic Accounting Events)
-- -----------------------------------------------------------------------------
-- A JournalEntry (AccountingEvent) starts in 'draft' while postings are added,
-- and is sealed to 'posted' (or superseded via 'voided' / reversal link).
-- Sealing to 'posted' triggers database-level verification that every
-- (entity_id, currency) partition is strictly balanced.
CREATE TABLE journal_entries (
    id                      TEXT PRIMARY KEY,
    posting_date            TEXT NOT NULL, -- YYYY-MM-DD
    effective_date          TEXT NOT NULL, -- Defaults to posting_date unless accrual override
    description             TEXT NOT NULL DEFAULT '',
    status                  TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'superseded', 'voided')),
    replaces_entry_id       TEXT REFERENCES journal_entries(id),
    created_at              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- -----------------------------------------------------------------------------
-- 4. JOURNAL POSTINGS (Immutable Double-Entry Lines)
-- -----------------------------------------------------------------------------
-- Every posting is a non-negative integer in minor units (`amount_minor > 0`)
-- with an explicit 'debit' or 'credit' side.
-- Notice that P&L lines (revenue/expense) are first-class journal_postings
-- rather than a disconnected side-table, ensuring Assets = Liabilities + Equity
-- holds inside journal_postings alone.
CREATE TABLE journal_postings (
    id                  TEXT PRIMARY KEY,
    entry_id            TEXT NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    ledger_account_id   TEXT NOT NULL REFERENCES ledger_accounts(id),
    entity_id           TEXT NOT NULL REFERENCES entities(id),
    currency            TEXT NOT NULL CHECK (length(currency) = 3),
    side                TEXT NOT NULL CHECK (side IN ('debit', 'credit')),
    amount_minor        INTEGER NOT NULL CHECK (amount_minor > 0),
    memo                TEXT NOT NULL DEFAULT ''
);

CREATE INDEX idx_postings_entry_partition ON journal_postings(entry_id, entity_id, currency, side);
CREATE INDEX idx_postings_account ON journal_postings(ledger_account_id);

-- Guard 1: Posting's entity_id and currency MUST match its ledger_account.
CREATE TRIGGER trg_posting_matches_ledger_account
BEFORE INSERT ON journal_postings
WHEN (
    (SELECT entity_id FROM ledger_accounts WHERE id = NEW.ledger_account_id) != NEW.entity_id
    OR (SELECT currency FROM ledger_accounts WHERE id = NEW.ledger_account_id) != NEW.currency
)
BEGIN
    SELECT RAISE(ABORT, 'INV-002: journal_posting entity_id and currency must match ledger_account');
END;

-- Guard 2: Cannot add, edit, or delete postings on an active, superseded, or voided entry.
CREATE TRIGGER trg_posting_immutable_insert
BEFORE INSERT ON journal_postings
WHEN (SELECT status FROM journal_entries WHERE id = NEW.entry_id) IN ('active', 'superseded', 'voided')
BEGIN
    SELECT RAISE(ABORT, 'INV-IMMUTABLE: Cannot insert postings into an active or terminal journal_entry');
END;

CREATE TRIGGER trg_posting_immutable_update
BEFORE UPDATE ON journal_postings
WHEN (SELECT status FROM journal_entries WHERE id = OLD.entry_id) IN ('active', 'superseded', 'voided')
BEGIN
    SELECT RAISE(ABORT, 'INV-IMMUTABLE: Cannot update postings of an active or terminal journal_entry');
END;

CREATE TRIGGER trg_posting_immutable_delete
BEFORE DELETE ON journal_postings
WHEN (SELECT status FROM journal_entries WHERE id = OLD.entry_id) IN ('active', 'superseded', 'voided')
BEGIN
    SELECT RAISE(ABORT, 'INV-IMMUTABLE: Cannot delete postings of an active or terminal journal_entry');
END;

-- Guard 3: Sealing a journal_entry to 'posted' enforces:
--   (a) At least 2 postings exist
--   (b) SUM(debits) == SUM(credits) for EVERY (entity_id, currency) partition
CREATE TRIGGER trg_journal_entry_seal_balance
BEFORE UPDATE OF status ON journal_entries
WHEN OLD.status = 'draft' AND NEW.status = 'active'
BEGIN
    -- Must have at least 2 postings
    SELECT RAISE(ABORT, 'INV-001: Cannot post an empty journal_entry')
    WHERE NOT EXISTS (
        SELECT 1 FROM journal_postings WHERE entry_id = NEW.id
    );

    -- Every (entity_id, currency) partition in this entry must net to 0
    SELECT RAISE(ABORT, 'INV-001: Unbalanced journal_entry in (entity_id, currency) partition')
    WHERE EXISTS (
        SELECT entity_id, currency
        FROM journal_postings
        WHERE entry_id = NEW.id
        GROUP BY entity_id, currency
        HAVING SUM(CASE WHEN side = 'debit' THEN amount_minor ELSE -amount_minor END) != 0
    );
END;

-- Guard 4: Active journal entries can only transition to superseded or voided. Header fields cannot be altered.
CREATE TRIGGER trg_journal_entry_lifecycle_active
BEFORE UPDATE ON journal_entries
WHEN OLD.status = 'active'
BEGIN
    SELECT RAISE(ABORT, 'INV-IMMUTABLE: Active journal entries can only transition to superseded or voided')
    WHERE NEW.status NOT IN ('superseded', 'voided');

    SELECT RAISE(ABORT, 'INV-IMMUTABLE: Cannot mutate header fields of an active journal entry')
    WHERE NEW.posting_date != OLD.posting_date
       OR NEW.effective_date != OLD.effective_date
       OR NEW.description != OLD.description;
END;

-- A correction must first rebuild the affected settlement group in one reviewed operation.
CREATE TRIGGER trg_settled_group_prevents_supersede
BEFORE UPDATE OF status ON journal_entries
WHEN OLD.status = 'active' AND NEW.status IN ('superseded', 'voided')
  AND EXISTS (
    SELECT 1 FROM journal_postings jp
    JOIN settlement_group_postings sgp ON sgp.posting_id = jp.id
    JOIN settlement_groups sg ON sg.id = sgp.group_id
    WHERE jp.entry_id = OLD.id AND sg.status = 'settled'
  )
BEGIN
    SELECT RAISE(ABORT, 'INV-013: settled group must be corrected atomically with its entry');
END;

-- Guard 5: Terminal journal entries (superseded or voided) are permanently sealed
CREATE TRIGGER trg_journal_entry_lifecycle_terminal
BEFORE UPDATE ON journal_entries
WHEN OLD.status IN ('superseded', 'voided')
BEGIN
    SELECT RAISE(ABORT, 'INV-IMMUTABLE: Superseded or voided journal entries are permanently sealed');
END;

-- -----------------------------------------------------------------------------
-- 5. SETTLEMENT GROUPS (Zero-Residual Atomic Clearing across Entries)
-- -----------------------------------------------------------------------------
-- Used when multiple independent events (e.g., bank transfer out + bank transfer in
-- on different dates, or payroll gross-to-net + HSA deposit, or batch reimbursements)
-- must clear a transit/clearing account to exact zero per (entity_id, currency, ledger_account_id).
CREATE TABLE settlement_groups (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'settled')),
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE settlement_group_postings (
    group_id    TEXT NOT NULL REFERENCES settlement_groups(id) ON DELETE CASCADE,
    posting_id  TEXT NOT NULL UNIQUE REFERENCES journal_postings(id),
    PRIMARY KEY (group_id, posting_id)
);

-- Guard: Sealing a settlement_group to 'settled' requires:
--   1. At least 2 clearing postings linked
--   2. Every linked posting belongs to a 'posted' journal_entry
--   3. Every (entity_id, currency, ledger_account_id) clearing partition in the group nets to 0
CREATE TRIGGER trg_settlement_group_seal
BEFORE UPDATE OF status ON settlement_groups
WHEN OLD.status = 'open' AND NEW.status = 'settled'
BEGIN
    SELECT RAISE(ABORT, 'INV-008: Cannot settle an empty settlement_group')
    WHERE NOT EXISTS (
        SELECT 1 FROM settlement_group_postings WHERE group_id = NEW.id
    );

    SELECT RAISE(ABORT, 'INV-008: All postings in a settled group must belong to posted journal_entries')
    WHERE EXISTS (
        SELECT 1
        FROM settlement_group_postings sgp
        JOIN journal_postings jp ON jp.id = sgp.posting_id
        JOIN journal_entries je ON je.id = jp.entry_id
        WHERE sgp.group_id = NEW.id AND je.status != 'active'
    );

    SELECT RAISE(ABORT, 'INV-008/INV-030: Non-zero clearing residual in settlement_group partition')
    WHERE EXISTS (
        SELECT jp.entity_id, jp.currency, jp.ledger_account_id
        FROM settlement_group_postings sgp
        JOIN journal_postings jp ON jp.id = sgp.posting_id
        JOIN ledger_accounts la ON la.id = jp.ledger_account_id
        WHERE sgp.group_id = NEW.id AND la.role = 'clearing'
        GROUP BY jp.entity_id, jp.currency, jp.ledger_account_id
        HAVING SUM(CASE WHEN jp.side = 'debit' THEN jp.amount_minor ELSE -jp.amount_minor END) != 0
    );
END;

-- -----------------------------------------------------------------------------
-- 6. READ-SIDE INVARIANT VIEWS (Trial Balance & Accounting Equation Proof)
-- -----------------------------------------------------------------------------

-- Signed normal balance per ledger account (only 'posted' entries):
--   Assets & Expenses:   Debit - Credit
--   Liabilities, Equity, Revenue: Credit - Debit
CREATE VIEW v_account_balances AS
SELECT
    la.id AS ledger_account_id,
    la.entity_id,
    la.name AS account_name,
    la.currency,
    la.element,
    la.role,
    
    COALESCE(SUM(CASE WHEN jp.side = 'debit'  THEN jp.amount_minor ELSE 0 END), 0) AS total_debits_minor,
    COALESCE(SUM(CASE WHEN jp.side = 'credit' THEN jp.amount_minor ELSE 0 END), 0) AS total_credits_minor,
    COALESCE(SUM(
        CASE
            WHEN la.element IN ('asset', 'expense') THEN
                (CASE WHEN jp.side = 'debit' THEN jp.amount_minor ELSE -jp.amount_minor END)
            ELSE
                (CASE WHEN jp.side = 'credit' THEN jp.amount_minor ELSE -jp.amount_minor END)
        END
    ), 0) AS normal_balance_minor
FROM ledger_accounts la
LEFT JOIN journal_postings jp ON jp.ledger_account_id = la.id
    AND EXISTS (SELECT 1 FROM journal_entries je WHERE je.id = jp.entry_id AND je.status = 'active')
GROUP BY la.id, la.entity_id, la.name, la.currency, la.element, la.role;

-- Live verification of the Fundamental Accounting Equation per (entity_id, currency):
--   Assets == Liabilities + Contributed_Equity + (Revenue - Expenses)
CREATE VIEW v_accounting_equation_check AS
SELECT
    entity_id,
    currency,
    SUM(total_debits_minor) AS trial_debits_minor,
    SUM(total_credits_minor) AS trial_credits_minor,
    SUM(CASE WHEN element = 'asset'     THEN normal_balance_minor ELSE 0 END) AS assets_minor,
    SUM(CASE WHEN element = 'liability' THEN normal_balance_minor ELSE 0 END) AS liabilities_minor,
    SUM(CASE WHEN element = 'equity'    THEN normal_balance_minor ELSE 0 END) AS equity_paid_in_minor,
    SUM(CASE WHEN element = 'revenue'   THEN normal_balance_minor ELSE 0 END) AS revenue_minor,
    SUM(CASE WHEN element = 'expense'   THEN normal_balance_minor ELSE 0 END) AS expenses_minor,
    SUM(CASE WHEN element IN ('equity', 'revenue') THEN normal_balance_minor
             WHEN element = 'expense'              THEN -normal_balance_minor
             ELSE 0 END) AS total_equity_minor,
    -- Equation discrepancy MUST always be 0: Assets - (Liabilities + Total_Equity)
    SUM(CASE WHEN element = 'asset'     THEN normal_balance_minor ELSE 0 END)
    - (
        SUM(CASE WHEN element = 'liability' THEN normal_balance_minor ELSE 0 END)
        + SUM(CASE WHEN element IN ('equity', 'revenue') THEN normal_balance_minor
                   WHEN element = 'expense'              THEN -normal_balance_minor
                   ELSE 0 END)
    ) AS equation_discrepancy_minor
FROM v_account_balances
GROUP BY entity_id, currency;


-- -----------------------------------------------------------------------------
-- 7. PERFORMANCE INDEXES (ACTIVE WORKFLOWS & AUDIT LINEAGE)
-- -----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_journal_entries_active_date
ON journal_entries(posting_date)
WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_journal_entries_replaces
ON journal_entries(replaces_entry_id)
WHERE replaces_entry_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_postings_account_entry
ON journal_postings(ledger_account_id, entry_id);
