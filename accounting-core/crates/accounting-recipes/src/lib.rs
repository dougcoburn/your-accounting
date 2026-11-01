//! Application Ring: Recipe Compilers (`RecipeCompiler`)
//!
//! Translates high-level UX business intents (cross-entity split purchases, transfers,
//! opaque-party receivables/reimbursements/forgiveness, cross-entity refunds,
//! payroll gross-to-net clearing) into balanced `JournalEntryDraft` aggregates
//! without modifying the 6-table Core Kernel.

use accounting_kernel::{
    AccountElement, JournalEntryDraft, KernelError, Money, PostingPrimitive, PostingSide,
};

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
    /// Compiles a single payment funded by `funding_entity_id` into expenses across
    /// one or more transparent entities, automatically generating paired inter-entity
    /// Due-From (`Asset`) and Due-To (`Liability`) postings.
    pub fn split_purchase_across_entities(
        posting_date: &str,
        funding_entity_id: &str,
        funding_account_id: &str,
        _funding_element: AccountElement,
        currency: &str,
        allocations: &[SplitAllocation<'_>],
        description: &str,
    ) -> Result<JournalEntryDraft, KernelError> {
        let mut total_minor: i64 = 0;
        for item in allocations {
            total_minor = total_minor
                .checked_add(item.amount_minor)
                .ok_or(KernelError::ArithmeticOverflow)?;
        }

        let mut postings = Vec::with_capacity(1 + allocations.len() * 3);

        // 1. Funding side payment:
        //    Paid from Asset (Checking) -> CREDIT decreases Asset
        //    Paid from Liability (Credit Card) -> CREDIT increases Liability
        postings.push(PostingPrimitive::new(
            funding_account_id,
            funding_entity_id,
            Money::new(total_minor, currency)?,
            PostingSide::Credit,
            format!("Funding payment: {description}"),
        ));

        // 2. Expense allocations + automatic inter-entity Due-From / Due-To obligations
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

                // Funding entity records an Asset (Due From target_entity)
                postings.push(PostingPrimitive::new(
                    due_from_acct,
                    funding_entity_id,
                    Money::new(alloc.amount_minor, currency)?,
                    PostingSide::Debit,
                    format!("Inter-entity receivable from {}", alloc.target_entity_id),
                ));

                // Target entity records its Expense (DEBIT) and Liability Due To funding_entity (CREDIT)
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

        Ok(JournalEntryDraft::new(
            posting_date,
            "recipe:split_purchase:v1",
            description,
            postings,
        ))
    }
}
