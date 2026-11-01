//! SQLite Infrastructure Adapter implementing the `LedgerStore` Port.

use std::path::Path;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{
    AccountElement, AccountRole, EntityVisibility, EquationSnapshot, JournalEntryDraft,
    KernelError, LedgerStore,
};

pub const KERNEL_SCHEMA_SQL: &str = include_str!("../../../schema/accounting-core-kernel-v1.sql");

pub struct SqliteLedgerAdapter {
    conn: Connection,
}

impl SqliteLedgerAdapter {
    /// Creates an in-memory SQLite adapter initialized with `accounting-core-kernel-v1.sql`.
    pub fn in_memory() -> Result<Self, KernelError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Self::from_connection(conn)
    }

    /// Opens or creates a file-backed SQLite adapter initialized with `accounting-core-kernel-v1.sql`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, KernelError> {
        let conn = Connection::open(path)
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Self::from_connection(conn)
    }

    pub fn from_connection(conn: Connection) -> Result<Self, KernelError> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        conn.execute_batch(KERNEL_SCHEMA_SQL)
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        Ok(Self { conn })
    }

    pub fn raw_connection(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

impl LedgerStore for SqliteLedgerAdapter {
    fn register_entity(
        &mut self,
        entity_id: &str,
        name: &str,
        visibility: EntityVisibility,
    ) -> Result<(), KernelError> {
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

    fn post_entry(&mut self, draft: &JournalEntryDraft) -> Result<String, KernelError> {
        draft.validate_trial_balance()?;
        let eff_date = draft
            .effective_date
            .as_deref()
            .unwrap_or(&draft.posting_date);

        let tx = self
            .conn
            .transaction()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

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

        // Sealing to 'posted' fires SQLite trigger `trg_journal_entry_seal_balance`
        tx.execute(
            "UPDATE journal_entries SET status = 'posted' WHERE id = ?1",
            params![draft.id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;

        tx.commit()
            .map_err(|e| KernelError::Storage(e.to_string()))?;

        Ok(draft.id.clone())
    }

    fn settle_group(
        &mut self,
        group_id: &str,
        name: &str,
        posting_ids: &[&str],
    ) -> Result<(), KernelError> {
        let tx = self
            .conn
            .transaction()
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

    fn verify_accounting_equation(
        &self,
        entity_id: &str,
        currency: &str,
    ) -> Result<EquationSnapshot, KernelError> {
        let snap: Option<EquationSnapshot> = self
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
                    Ok(EquationSnapshot {
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

        let snap = snap.ok_or_else(|| KernelError::UnknownEntityPartition {
            entity_id: entity_id.to_string(),
            currency: currency.to_string(),
        })?;

        if !snap.is_balanced() {
            return Err(KernelError::EquationViolation(snap));
        }
        Ok(snap)
    }
}
