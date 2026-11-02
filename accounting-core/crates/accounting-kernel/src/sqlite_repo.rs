//! SQLite Infrastructure Adapter implementing the `LedgerStore` Port.
//!
//! Enforces Design-by-Contract (DbC) preconditions before executing SQL so that
//! callers always receive deterministic, typed `KernelError` variants rather than
//! raw SQLite constraint strings, while keeping SQLite triggers active as
//! defense-in-depth.

use std::collections::BTreeMap;
use std::path::Path;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{
    AccountElement, AccountRole, AccountingEquationSummary, CommittedJournalEntry, Currency,
    EntityVisibility, EntryFilter, EntryStatus, JournalEntryDraft, KernelError, LedgerAccount,
    LedgerStore, Money, PostingPrimitive, PostingSide,
};

pub const KERNEL_SCHEMA_SQL: &str = include_str!("../../../schema/accounting-core-kernel-v1.sql");

pub struct SqliteLedgerAdapter {
    conn: Connection,
}

/// An application-owned commit boundary. Dropping it rolls back all staged writes.
pub struct SqliteLedgerTransaction<'a> {
    store: &'a mut SqliteLedgerAdapter,
    expected_revision: i64,
    finished: bool,
    failed: bool,
}

impl SqliteLedgerTransaction<'_> {
    /// Temporary bridge for application repositories until they have storage ports.
    /// A failed operation poisons the transaction and prevents a partial commit.
    pub fn with_connection<T>(
        &mut self,
        operation: impl FnOnce(&mut Connection) -> Result<T, KernelError>,
    ) -> Result<T, KernelError> {
        let result = operation(&mut self.store.conn);
        self.failed |= result.is_err();
        result
    }

    pub fn commit(mut self) -> Result<i64, KernelError> {
        if self.failed {
            return Err(KernelError::TransactionAborted);
        }
        SqliteLedgerAdapter::validate_aggregate_bounds(&self.store.conn)?;
        let changed = self.store.conn.execute(
            "UPDATE kernel_state SET revision = revision + 1 WHERE id = 1 AND revision = ?1",
            params![self.expected_revision],
        ).map_err(|e| KernelError::Storage(e.to_string()))?;
        if changed != 1 {
            return Err(KernelError::RevisionConflict {
                expected: self.expected_revision,
                actual: self.store.revision()?,
            });
        }
        self.store.conn.execute_batch("COMMIT")
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        self.finished = true;
        Ok(self.expected_revision + 1)
    }
}

impl Drop for SqliteLedgerTransaction<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.store.conn.execute_batch("ROLLBACK");
        }
    }
}

impl LedgerStore for SqliteLedgerTransaction<'_> {
    fn register_entity(&mut self, id: &str, name: &str, visibility: EntityVisibility) -> Result<(), KernelError> {
        let result = self.store.register_entity(id, name, visibility);
        self.failed |= result.is_err();
        result
    }
    fn create_ledger_account(&mut self, id: &str, entity: &str, name: &str, currency: &str, element: AccountElement, role: AccountRole) -> Result<(), KernelError> {
        let result = self.store.create_ledger_account(id, entity, name, currency, element, role);
        self.failed |= result.is_err();
        result
    }
    fn get_account(&self, id: &str) -> Result<Option<LedgerAccount>, KernelError> {
        self.store.get_account(id)
    }
    fn list_accounts(&self, entity: &str) -> Result<Vec<LedgerAccount>, KernelError> {
        self.store.list_accounts(entity)
    }
    fn post_entry(&mut self, draft: &JournalEntryDraft) -> Result<String, KernelError> {
        let result = self.store.post_entry(draft);
        self.failed |= result.is_err();
        result
    }
    fn get_entry(&self, id: &str) -> Result<Option<CommittedJournalEntry>, KernelError> {
        self.store.get_entry(id)
    }
    fn list_active_entries_for_account(&self, id: &str, filter: &EntryFilter) -> Result<Vec<CommittedJournalEntry>, KernelError> {
        self.store.list_active_entries_for_account(id, filter)
    }
    fn get_entry_lineage(&self, id: &str) -> Result<Vec<CommittedJournalEntry>, KernelError> {
        self.store.get_entry_lineage(id)
    }
    fn settle_group(&mut self, id: &str, name: &str, posting_ids: &[&str]) -> Result<(), KernelError> {
        let result = self.store.settle_group(id, name, posting_ids);
        self.failed |= result.is_err();
        result
    }
    fn get_account_balance(&self, id: &str) -> Result<i64, KernelError> {
        self.store.get_account_balance(id)
    }
    fn summarize_accounting_equation(&self, entity: &str, currency: &str) -> Result<AccountingEquationSummary, KernelError> {
        self.store.summarize_accounting_equation(entity, currency)
    }
}

impl SqliteLedgerAdapter {
    fn validate_aggregate_bounds(conn: &Connection) -> Result<(), KernelError> {
        let mut stmt = conn.prepare(
            "SELECT trial_debits_minor, trial_credits_minor, assets_minor,
                    liabilities_minor, total_equity_minor, equation_discrepancy_minor
             FROM v_accounting_equation_check",
        ).map_err(|e| KernelError::Storage(e.to_string()))?;
        let mut rows = stmt.query([]).map_err(|e| KernelError::Storage(e.to_string()))?;
        while let Some(row) = rows.next().map_err(|_| KernelError::ArithmeticOverflow)? {
            for index in 0..6 {
                let _: i64 = row.get(index).map_err(|_| KernelError::ArithmeticOverflow)?;
            }
        }
        Ok(())
    }

    pub fn in_memory() -> Result<Self, KernelError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Self::from_connection(conn)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, KernelError> {
        let conn = Connection::open(path)
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Self::from_connection(conn)
    }

    pub fn from_connection(mut conn: Connection) -> Result<Self, KernelError> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        if version > 3 {
            return Err(KernelError::Storage(format!("unsupported kernel schema version {version}")));
        }
        if version < 3 {
            let has_schema: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'entities')",
                [], |row| row.get(0),
            ).map_err(|e| KernelError::Storage(e.to_string()))?;
            let tx = conn.transaction().map_err(|e| KernelError::Storage(e.to_string()))?;
            if version == 0 && has_schema == 0 {
                tx.execute_batch(KERNEL_SCHEMA_SQL)
                    .map_err(|e| KernelError::Storage(e.to_string()))?;
            }
            tx.execute_batch("CREATE TABLE IF NOT EXISTS kernel_state (id INTEGER PRIMARY KEY CHECK(id = 1), revision INTEGER NOT NULL CHECK(revision >= 0));
                INSERT OR IGNORE INTO kernel_state (id, revision) VALUES (1, 0);
                CREATE TRIGGER IF NOT EXISTS trg_settled_group_prevents_supersede
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
                DROP VIEW IF EXISTS v_account_balances;
                CREATE VIEW v_account_balances AS
                SELECT la.id AS ledger_account_id, la.entity_id, la.name AS account_name,
                       la.currency, la.element, la.role,
                       COALESCE(SUM(CASE WHEN jp.side = 'debit' THEN jp.amount_minor ELSE 0 END), 0) AS total_debits_minor,
                       COALESCE(SUM(CASE WHEN jp.side = 'credit' THEN jp.amount_minor ELSE 0 END), 0) AS total_credits_minor,
                       COALESCE(SUM(CASE
                           WHEN la.element IN ('asset', 'expense') THEN
                               CASE WHEN jp.side = 'debit' THEN jp.amount_minor ELSE -jp.amount_minor END
                           ELSE CASE WHEN jp.side = 'credit' THEN jp.amount_minor ELSE -jp.amount_minor END
                       END), 0) AS normal_balance_minor
                FROM ledger_accounts la
                LEFT JOIN journal_postings jp ON jp.ledger_account_id = la.id
                    AND EXISTS (SELECT 1 FROM journal_entries je WHERE je.id = jp.entry_id AND je.status = 'active')
                GROUP BY la.id, la.entity_id, la.name, la.currency, la.element, la.role;
                PRAGMA user_version = 3;")
                .map_err(|e| KernelError::Storage(e.to_string()))?;
            tx.commit().map_err(|e| KernelError::Storage(e.to_string()))?;
        }
        Ok(Self { conn })
    }

    pub fn revision(&self) -> Result<i64, KernelError> {
        self.conn.query_row("SELECT revision FROM kernel_state WHERE id = 1", [], |row| row.get(0))
            .map_err(|e| KernelError::Storage(e.to_string()))
    }

    /// Begin a shared SQLite transaction after checking the revision shown in a preview.
    pub fn begin_transaction(&mut self, expected_revision: i64) -> Result<SqliteLedgerTransaction<'_>, KernelError> {
        self.conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        let actual = match self.revision() {
            Ok(actual) => actual,
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        };
        if actual != expected_revision {
            let _ = self.conn.execute_batch("ROLLBACK");
            return Err(KernelError::RevisionConflict { expected: expected_revision, actual });
        }
        Ok(SqliteLedgerTransaction { store: self, expected_revision, finished: false, failed: false })
    }

    pub fn raw_connection(&mut self) -> &mut Connection {
        &mut self.conn
    }

    fn fetch_postings_for_entry(&self, entry_id: &str) -> Result<Vec<PostingPrimitive>, KernelError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, ledger_account_id, entity_id, currency, side, amount_minor, memo
                 FROM journal_postings
                 WHERE entry_id = ?1
                 ORDER BY rowid ASC",
            )
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map(params![entry_id], |row| {
                let id: String = row.get(0)?;
                let ledger_account_id: String = row.get(1)?;
                let entity_id: String = row.get(2)?;
                let currency_str: String = row.get(3)?;
                let side_str: String = row.get(4)?;
                let amount_minor: i64 = row.get(5)?;
                let memo: String = row.get(6)?;
                Ok((id, ledger_account_id, entity_id, currency_str, side_str, amount_minor, memo))
            })
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        let mut postings = Vec::new();
        for item in rows {
            let (id, ledger_account_id, entity_id, curr, side_str, amount_minor, memo) =
                item.map_err(|e| KernelError::Storage(e.to_string()))?;
            let side = PostingSide::parse(&side_str)?;
            let money = Money::new(amount_minor, &curr)?;
            postings.push(PostingPrimitive {
                id,
                ledger_account_id,
                entity_id,
                money,
                side,
                memo,
            });
        }
        Ok(postings)
    }
}

impl LedgerStore for SqliteLedgerAdapter {
    fn register_entity(
        &mut self,
        entity_id: &str,
        name: &str,
        visibility: EntityVisibility,
    ) -> Result<(), KernelError> {
        let exists: Option<i32> = self
            .conn
            .query_row(
                "SELECT 1 FROM entities WHERE id = ?1",
                params![entity_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        if exists.is_some() {
            return Err(KernelError::DuplicateId {
                record_kind: "entity",
                id: entity_id.to_string(),
            });
        }

        self.conn
            .execute(
                "INSERT INTO entities (id, name, visibility) VALUES (?1, ?2, ?3)",
                params![entity_id, name, visibility.as_str()],
            )
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Ok(())
    }

    fn create_ledger_account(
        &mut self,
        account_id: &str,
        entity_id: &str,
        name: &str,
        currency: &str,
        element: AccountElement,
        role: AccountRole,
    ) -> Result<(), KernelError> {
        // 1. Validate currency format
        let _ = Currency::parse(currency)?;

        // 2. Check duplicate account_id
        if self.get_account(account_id)?.is_some() {
            return Err(KernelError::DuplicateId {
                record_kind: "ledger_account",
                id: account_id.to_string(),
            });
        }

        // 3. Check entity exists and is Transparent
        let vis_str: Option<String> = self
            .conn
            .query_row(
                "SELECT visibility FROM entities WHERE id = ?1",
                params![entity_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        match vis_str.as_deref() {
            Some("transparent") => {}
            Some(_) => {
                return Err(KernelError::AccountOwnerNotTransparent {
                    entity_id: entity_id.to_string(),
                });
            }
            None => return Err(KernelError::UnknownEntity(entity_id.to_string())),
        }

        // 4. Clearing accounts must be Asset or Liability
        if role == AccountRole::Clearing
            && !matches!(element, AccountElement::Asset | AccountElement::Liability)
        {
            return Err(KernelError::InvalidClearingElement {
                account_id: account_id.to_string(),
                element,
            });
        }

        self.conn
            .execute(
                "INSERT INTO ledger_accounts
                    (id, entity_id, name, currency, element, role)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    account_id,
                    entity_id,
                    name,
                    currency,
                    element.as_str(),
                    role.as_str(),
                ],
            )
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Ok(())
    }

    fn get_account(&self, account_id: &str) -> Result<Option<LedgerAccount>, KernelError> {
        let res = self
            .conn
            .query_row(
                "SELECT id, entity_id, name, currency, element, role, is_active
                 FROM ledger_accounts
                 WHERE id = ?1",
                params![account_id],
                |row| {
                    let id: String = row.get(0)?;
                    let entity_id: String = row.get(1)?;
                    let name: String = row.get(2)?;
                    let currency_str: String = row.get(3)?;
                    let element_str: String = row.get(4)?;
                    let role_str: String = row.get(5)?;
                    let is_active_int: i32 = row.get(6)?;
                    Ok((id, entity_id, name, currency_str, element_str, role_str, is_active_int == 1))
                },
            )
            .optional()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        match res {
            Some((id, entity_id, name, curr_str, elem_str, role_str, is_active)) => {
                let currency = Currency::parse(&curr_str)?;
                let element = AccountElement::parse(&elem_str)?;
                let role = AccountRole::parse(&role_str)?;
                Ok(Some(LedgerAccount {
                    id,
                    entity_id,
                    name,
                    currency,
                    element,
                    role,
                    is_active,
                }))
            }
            None => Ok(None),
        }
    }

    fn list_accounts(&self, entity_id: &str) -> Result<Vec<LedgerAccount>, KernelError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, entity_id, name, currency, element, role, is_active
                 FROM ledger_accounts
                 WHERE entity_id = ?1
                 ORDER BY element ASC, name ASC",
            )
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map(params![entity_id], |row| {
                let id: String = row.get(0)?;
                let ent_id: String = row.get(1)?;
                let name: String = row.get(2)?;
                let currency_str: String = row.get(3)?;
                let element_str: String = row.get(4)?;
                let role_str: String = row.get(5)?;
                let is_active_int: i32 = row.get(6)?;
                Ok((id, ent_id, name, currency_str, element_str, role_str, is_active_int == 1))
            })
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        let mut accounts = Vec::new();
        for item in rows {
            let (id, ent_id, name, curr_str, elem_str, role_str, is_active) =
                item.map_err(|e| KernelError::Storage(e.to_string()))?;
            let currency = Currency::parse(&curr_str)?;
            let element = AccountElement::parse(&elem_str)?;
            let role = AccountRole::parse(&role_str)?;
            accounts.push(LedgerAccount {
                id,
                entity_id: ent_id,
                name,
                currency,
                element,
                role,
                is_active,
            });
        }
        Ok(accounts)
    }

    fn post_entry(&mut self, draft: &JournalEntryDraft) -> Result<String, KernelError> {
        // 1. Validate trial balance partitions (INV-001)
        draft.validate_trial_balance()?;
        let standalone = self.conn.is_autocommit();

        // 2. Check duplicate entry ID
        if self.get_entry(&draft.id)?.is_some() {
            return Err(KernelError::DuplicateId {
                record_kind: "journal_entry",
                id: draft.id.clone(),
            });
        }

        // 3. Validate each posting against its target LedgerAccount (INV-002)
        for p in &draft.postings {
            let acct = self
                .get_account(&p.ledger_account_id)?
                .ok_or_else(|| KernelError::UnknownLedgerAccount(p.ledger_account_id.clone()))?;

            if acct.entity_id != p.entity_id || acct.currency != p.money.currency() {
                return Err(KernelError::PostingAccountMismatch {
                    posting_id: p.id.clone(),
                    ledger_account_id: p.ledger_account_id.clone(),
                    expected_entity_id: acct.entity_id,
                    actual_entity_id: p.entity_id.clone(),
                    expected_currency: acct.currency.as_str().to_string(),
                    actual_currency: p.money.currency().as_str().to_string(),
                });
            }
        }

        let eff_date = draft
            .effective_date
            .as_deref()
            .unwrap_or(&draft.posting_date);

        let tx = self
            .conn
            .savepoint()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        // 4. If replacing an existing entry, verify it exists and is Active, then supersede it
        if let Some(ref old_id) = draft.replaces_entry_id {
            let old_status: Option<String> = tx
                .query_row(
                    "SELECT status FROM journal_entries WHERE id = ?1",
                    params![old_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| KernelError::Storage(e.to_string()))?;

            match old_status.as_deref() {
                Some("active") => {
                    let settled_group: Option<String> = tx
                        .query_row(
                            "SELECT sg.id FROM settlement_groups sg
                             JOIN settlement_group_postings sgp ON sgp.group_id = sg.id
                             JOIN journal_postings jp ON jp.id = sgp.posting_id
                             WHERE jp.entry_id = ?1 AND sg.status = 'settled' LIMIT 1",
                            params![old_id],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(|e| KernelError::Storage(e.to_string()))?;
                    if let Some(group_id) = settled_group {
                        return Err(KernelError::InvalidSupersedeTarget {
                            entry_id: old_id.clone(),
                            reason: format!("entry participates in settled group {group_id}"),
                        });
                    }
                    tx.execute(
                        "UPDATE journal_entries SET status = 'superseded' WHERE id = ?1",
                        params![old_id],
                    )
                    .map_err(|e| KernelError::Storage(e.to_string()))?;
                }
                Some(other) => {
                    return Err(KernelError::InvalidSupersedeTarget {
                        entry_id: old_id.clone(),
                        reason: format!("target entry has status '{other}', expected 'active'"),
                    });
                }
                None => {
                    return Err(KernelError::UnknownEntry(old_id.clone()));
                }
            }
        }

        // 5. Insert new entry as 'draft', attach postings, and seal to 'active'
        tx.execute(
            "INSERT INTO journal_entries
                (id, posting_date, effective_date, description, status, replaces_entry_id)
             VALUES (?1, ?2, ?3, ?4, 'draft', ?5)",
            params![
                draft.id,
                draft.posting_date,
                eff_date,
                draft.description,
                draft.replaces_entry_id
            ],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;

        for p in &draft.postings {
            tx.execute(
                "INSERT INTO journal_postings
                    (id, entry_id, ledger_account_id, entity_id, currency, side, amount_minor, memo)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    p.id,
                    draft.id,
                    p.ledger_account_id,
                    p.entity_id,
                    p.money.currency().as_str(),
                    p.side.as_str(),
                    p.money.amount_minor(),
                    p.memo
                ],
            )
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        }

        tx.execute(
            "UPDATE journal_entries SET status = 'active' WHERE id = ?1",
            params![draft.id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;

        if standalone {
            // Reject a mathematically balanced entry if it overflows an existing total.
            Self::validate_aggregate_bounds(&tx)?;
            tx.execute("UPDATE kernel_state SET revision = revision + 1 WHERE id = 1", [])
                .map_err(|e| KernelError::Storage(e.to_string()))?;
        }
        tx.commit()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        Ok(draft.id.clone())
    }

    fn get_entry(&self, entry_id: &str) -> Result<Option<CommittedJournalEntry>, KernelError> {
        let entry_meta = self
            .conn
            .query_row(
                "SELECT id, posting_date, effective_date, description, status, replaces_entry_id, created_at
                 FROM journal_entries
                 WHERE id = ?1",
                params![entry_id],
                |row| {
                    let id: String = row.get(0)?;
                    let posting_date: String = row.get(1)?;
                    let effective_date: String = row.get(2)?;
                    let description: String = row.get(3)?;
                    let status_str: String = row.get(4)?;
                    let replaces_entry_id: Option<String> = row.get(5)?;
                    let created_at: String = row.get(6)?;
                    Ok((id, posting_date, effective_date, description, status_str, replaces_entry_id, created_at))
                },
            )
            .optional()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        match entry_meta {
            Some((id, posting_date, effective_date, description, status_str, replaces_entry_id, created_at)) => {
                let status = EntryStatus::parse(&status_str)?;
                let postings = self.fetch_postings_for_entry(&id)?;
                Ok(Some(CommittedJournalEntry {
                    id,
                    posting_date,
                    effective_date,
                    description,
                    status,
                    replaces_entry_id,
                    created_at,
                    postings,
                }))
            }
            None => Ok(None),
        }
    }

    fn list_active_entries_for_account(
        &self,
        account_id: &str,
        filter: &EntryFilter,
    ) -> Result<Vec<CommittedJournalEntry>, KernelError> {
        let mut query = String::from(
            "SELECT DISTINCT je.id, je.posting_date, je.effective_date, je.description, je.status, je.replaces_entry_id, je.created_at
             FROM journal_entries je
             JOIN journal_postings jp ON jp.entry_id = je.id
             WHERE jp.ledger_account_id = ?1 AND je.status = 'active'"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(account_id.to_string())];

        if let Some(ref start) = filter.start_date {
            query.push_str(" AND je.posting_date >= ?");
            params_vec.push(Box::new(start.clone()));
        }

        if let Some(ref end) = filter.end_date {
            query.push_str(" AND je.posting_date <= ?");
            params_vec.push(Box::new(end.clone()));
        }

        query.push_str(" ORDER BY je.posting_date ASC, je.created_at ASC");

        if let Some(limit) = filter.limit {
            query.push_str(&format!(" LIMIT {limit}"));
            if let Some(offset) = filter.offset {
                query.push_str(&format!(" OFFSET {offset}"));
            }
        }

        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        let params_slice: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let rows = stmt
            .query_map(params_slice.as_slice(), |row| {
                let id: String = row.get(0)?;
                let posting_date: String = row.get(1)?;
                let effective_date: String = row.get(2)?;
                let description: String = row.get(3)?;
                let status_str: String = row.get(4)?;
                let replaces_entry_id: Option<String> = row.get(5)?;
                let created_at: String = row.get(6)?;
                Ok((id, posting_date, effective_date, description, status_str, replaces_entry_id, created_at))
            })
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        let mut entries = Vec::new();
        for item in rows {
            let (id, posting_date, effective_date, description, status_str, replaces_entry_id, created_at) =
                item.map_err(|e| KernelError::Storage(e.to_string()))?;
            let status = EntryStatus::parse(&status_str)?;
            let postings = self.fetch_postings_for_entry(&id)?;
            entries.push(CommittedJournalEntry {
                id,
                posting_date,
                effective_date,
                description,
                status,
                replaces_entry_id,
                created_at,
                postings,
            });
        }
        Ok(entries)
    }

    fn get_entry_lineage(&self, entry_id: &str) -> Result<Vec<CommittedJournalEntry>, KernelError> {
        let mut root_id = entry_id.to_string();
        loop {
            let parent_id: Option<String> = self
                .conn
                .query_row(
                    "SELECT replaces_entry_id FROM journal_entries WHERE id = ?1",
                    params![root_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| KernelError::Storage(e.to_string()))?
                .flatten();

            match parent_id {
                Some(p) => root_id = p,
                None => break,
            }
        }

        let mut lineage = Vec::new();
        let mut curr_id = Some(root_id);

        while let Some(id) = curr_id {
            if let Some(entry) = self.get_entry(&id)? {
                lineage.push(entry);
            }
            curr_id = self
                .conn
                .query_row(
                    "SELECT id FROM journal_entries WHERE replaces_entry_id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| KernelError::Storage(e.to_string()))?;
        }

        Ok(lineage)
    }

    fn settle_group(
        &mut self,
        group_id: &str,
        name: &str,
        posting_ids: &[&str],
    ) -> Result<(), KernelError> {
        let standalone = self.conn.is_autocommit();
        if posting_ids.is_empty() {
            return Err(KernelError::EmptySettlementGroup {
                group_id: group_id.to_string(),
            });
        }

        let group_exists: Option<i32> = self
            .conn
            .query_row(
                "SELECT 1 FROM settlement_groups WHERE id = ?1",
                params![group_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        if group_exists.is_some() {
            return Err(KernelError::DuplicateId {
                record_kind: "settlement_group",
                id: group_id.to_string(),
            });
        }

        // Validate every posting exists, is not already settled, belongs to an Active entry,
        // and compute clearing residuals per (entity_id, currency, ledger_account_id)
        let mut clearing_residuals: BTreeMap<(String, String, String), i64> = BTreeMap::new();

        for &pid in posting_ids {
            let already_settled: Option<i32> = self
                .conn
                .query_row(
                    "SELECT 1 FROM settlement_group_postings WHERE posting_id = ?1",
                    params![pid],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| KernelError::Storage(e.to_string()))?;

            if already_settled.is_some() {
                return Err(KernelError::PostingAlreadySettled {
                    posting_id: pid.to_string(),
                });
            }

            let row_info = self
                .conn
                .query_row(
                    "SELECT jp.entity_id, jp.currency, jp.ledger_account_id, jp.side, jp.amount_minor,
                            la.role, je.status
                     FROM journal_postings jp
                     JOIN ledger_accounts la ON la.id = jp.ledger_account_id
                     JOIN journal_entries je ON je.id = jp.entry_id
                     WHERE jp.id = ?1",
                    params![pid],
                    |row| {
                        let entity_id: String = row.get(0)?;
                        let currency: String = row.get(1)?;
                        let ledger_account_id: String = row.get(2)?;
                        let side_str: String = row.get(3)?;
                        let amount_minor: i64 = row.get(4)?;
                        let role_str: String = row.get(5)?;
                        let status_str: String = row.get(6)?;
                        Ok((entity_id, currency, ledger_account_id, side_str, amount_minor, role_str, status_str))
                    },
                )
                .optional()
                .map_err(|e| KernelError::Storage(e.to_string()))?;

            let (entity_id, currency, ledger_account_id, side_str, amount_minor, role_str, status_str) =
                row_info.ok_or_else(|| KernelError::UnknownPosting(pid.to_string()))?;

            let entry_status = EntryStatus::parse(&status_str)?;
            if entry_status != EntryStatus::Active {
                return Err(KernelError::SettlementPostingNotActive {
                    group_id: group_id.to_string(),
                    posting_id: pid.to_string(),
                    entry_status,
                });
            }

            if role_str == "clearing" {
                let signed = if side_str == "debit" {
                    amount_minor
                } else {
                    -amount_minor
                };
                let key = (entity_id, currency, ledger_account_id);
                let cur = clearing_residuals.entry(key).or_insert(0);
                *cur = cur.checked_add(signed).ok_or(KernelError::ArithmeticOverflow)?;
            }
        }

        for ((entity_id, currency, ledger_account_id), residual_minor) in clearing_residuals {
            if residual_minor != 0 {
                return Err(KernelError::NonZeroClearingResidual {
                    group_id: group_id.to_string(),
                    entity_id,
                    currency,
                    ledger_account_id,
                    residual_minor,
                });
            }
        }

        let tx = self
            .conn
            .savepoint()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        tx.execute(
            "INSERT INTO settlement_groups (id, name, status) VALUES (?1, ?2, 'open')",
            params![group_id, name],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;

        for pid in posting_ids {
            tx.execute(
                "INSERT INTO settlement_group_postings (group_id, posting_id) VALUES (?1, ?2)",
                params![group_id, pid],
            )
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        }

        tx.execute(
            "UPDATE settlement_groups SET status = 'settled' WHERE id = ?1",
            params![group_id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;

        if standalone {
            tx.execute("UPDATE kernel_state SET revision = revision + 1 WHERE id = 1", [])
                .map_err(|e| KernelError::Storage(e.to_string()))?;
        }
        tx.commit()
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Ok(())
    }

    fn get_account_balance(&self, account_id: &str) -> Result<i64, KernelError> {
        let bal: Option<i64> = self
            .conn
            .query_row(
                "SELECT normal_balance_minor FROM v_account_balances WHERE ledger_account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        bal.ok_or_else(|| KernelError::UnknownLedgerAccount(account_id.to_string()))
    }

    fn summarize_accounting_equation(
        &self,
        entity_id: &str,
        currency: &str,
    ) -> Result<AccountingEquationSummary, KernelError> {
        let snap: Option<AccountingEquationSummary> = self
            .conn
            .query_row(
                "SELECT
                    entity_id,
                    currency,
                    trial_debits_minor,
                    trial_credits_minor,
                    assets_minor,
                    liabilities_minor,
                    equity_paid_in_minor,
                    revenue_minor,
                    expenses_minor,
                    total_equity_minor,
                    equation_discrepancy_minor
                 FROM v_accounting_equation_check
                 WHERE entity_id = ?1 AND currency = ?2",
                params![entity_id, currency],
                |row| {
                    Ok(AccountingEquationSummary {
                        entity_id: row.get(0)?,
                        currency: row.get(1)?,
                        trial_debits_minor: row.get(2)?,
                        trial_credits_minor: row.get(3)?,
                        assets_minor: row.get(4)?,
                        liabilities_minor: row.get(5)?,
                        equity_paid_in_minor: row.get(6)?,
                        revenue_minor: row.get(7)?,
                        expenses_minor: row.get(8)?,
                        total_equity_minor: row.get(9)?,
                        equation_discrepancy_minor: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        let summary = snap.ok_or_else(|| KernelError::UnknownEntityPartition {
            entity_id: entity_id.to_string(),
            currency: currency.to_string(),
        })?;

        if !summary.is_balanced() {
            return Err(KernelError::KernelBugcheck {
                entity_id: entity_id.to_string(),
                currency: currency.to_string(),
                discrepancy_minor: summary.equation_discrepancy_minor,
                details: format!(
                    "FATAL INVARIANT BREAK: Assets({}) != Liabilities({}) + Equity({}). Immediate rollback.",
                    summary.assets_minor, summary.liabilities_minor, summary.total_equity_minor
                ),
            });
        }
        Ok(summary)
    }
}
