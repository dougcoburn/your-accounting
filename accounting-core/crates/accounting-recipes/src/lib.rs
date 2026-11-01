//! Application Ring: Recipe Compilers & Execution Provenance (`accounting-recipes`)

use accounting_kernel::{
    next_deterministic_id, AccountElement, JournalEntryDraft, KernelError, LedgerStore, Money,
    PostingPrimitive, PostingSide,
};
use rusqlite::{params, Connection};

pub const RECIPES_SCHEMA_SQL: &str = include_str!("../schema/accounting-recipes-v1.sql");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeExecution {
    pub id: String,
    pub recipe_name: String,
    pub recipe_version: String,
    pub journal_entry_id: Option<String>,
    pub parameters_json: String,
    pub executed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecipeOutput {
    pub draft: JournalEntryDraft,
    pub execution: RecipeExecution,
}

#[derive(Debug, Clone)]
pub struct SplitAllocation<'a> {
    pub target_entity_id: &'a str,
    pub expense_account_id: &'a str,
    pub amount_minor: i64,
    pub funding_due_from_account_id: Option<&'a str>,
    pub target_due_to_account_id: Option<&'a str>,
}

pub struct RecipeCompiler;

impl RecipeCompiler {
    pub fn split_purchase_across_entities(
        posting_date: &str,
        funding_entity_id: &str,
        funding_account_id: &str,
        _funding_element: AccountElement,
        currency: &str,
        allocations: &[SplitAllocation<'_>],
        description: &str,
    ) -> Result<RecipeOutput, KernelError> {
        let mut total_minor: i64 = 0;
        for item in allocations {
            total_minor = total_minor
                .checked_add(item.amount_minor)
                .ok_or(KernelError::ArithmeticOverflow)?;
        }

        let mut postings = Vec::with_capacity(1 + allocations.len() * 3);

        postings.push(PostingPrimitive::new(
            funding_account_id,
            funding_entity_id,
            Money::new(total_minor, currency)?,
            PostingSide::Credit,
            format!("Funding payment: {description}"),
        ));

        for alloc in allocations {
            if alloc.target_entity_id == funding_entity_id {
                postings.push(PostingPrimitive::new(
                    alloc.expense_account_id,
                    funding_entity_id,
                    Money::new(alloc.amount_minor, currency)?,
                    PostingSide::Debit,
                    format!("Direct expense: {description}"),
                ));
            } else {
                let due_from_acct = alloc.funding_due_from_account_id.ok_or_else(|| {
                    KernelError::MissingInterEntityAccounts {
                        target_entity_id: alloc.target_entity_id.to_string(),
                    }
                })?;
                let due_to_acct = alloc.target_due_to_account_id.ok_or_else(|| {
                    KernelError::MissingInterEntityAccounts {
                        target_entity_id: alloc.target_entity_id.to_string(),
                    }
                })?;

                postings.push(PostingPrimitive::new(
                    due_from_acct,
                    funding_entity_id,
                    Money::new(alloc.amount_minor, currency)?,
                    PostingSide::Debit,
                    format!("Inter-entity receivable from {}", alloc.target_entity_id),
                ));

                postings.push(PostingPrimitive::new(
                    alloc.expense_account_id,
                    alloc.target_entity_id,
                    Money::new(alloc.amount_minor, currency)?,
                    PostingSide::Debit,
                    format!("Allocated expense paid by {funding_entity_id}"),
                ));
                postings.push(PostingPrimitive::new(
                    due_to_acct,
                    alloc.target_entity_id,
                    Money::new(alloc.amount_minor, currency)?,
                    PostingSide::Credit,
                    format!("Inter-entity payable to {funding_entity_id}"),
                ));
            }
        }

        let draft = JournalEntryDraft::new(posting_date, description, postings);

        let params_json = format!(
            r#"{{"funding_entity":"{}","funding_account":"{}","currency":"{}","total_minor":{},"allocation_count":{}}}"#,
            funding_entity_id, funding_account_id, currency, total_minor, allocations.len()
        );

        let execution = RecipeExecution {
            id: next_deterministic_id("rx"),
            recipe_name: "split_purchase".to_string(),
            recipe_version: "1.0.0".to_string(),
            journal_entry_id: Some(draft.id.clone()),
            parameters_json: params_json,
            executed_at: None,
        };

        Ok(RecipeOutput { draft, execution })
    }

    pub fn post_recipe<S: LedgerStore>(
        store: &mut S,
        output: &RecipeOutput,
    ) -> Result<String, KernelError> {
        store.post_entry(&output.draft)
    }

    pub fn record_execution(
        conn: &Connection,
        execution: &RecipeExecution,
        committed_entry_id: &str,
    ) -> Result<(), KernelError> {
        conn.execute(
            "INSERT INTO recipe_executions
                (id, recipe_name, recipe_version, journal_entry_id, parameters_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                execution.id,
                execution.recipe_name,
                execution.recipe_version,
                committed_entry_id,
                execution.parameters_json
            ],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;
        Ok(())
    }
}
