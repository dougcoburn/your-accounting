//! M0 CLI Fixture Runner: executes core scenarios via the abstract `LedgerStore` Port.

use accounting_kernel::{
    AccountElement, AccountRole, EntityVisibility, JournalEntryDraft, LedgerStore, Money,
    PostingPrimitive, PostingSide, SqliteLedgerAdapter,
};
use accounting_recipes::{RecipeCompiler, SplitAllocation, RECIPES_SCHEMA_SQL};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = SqliteLedgerAdapter::in_memory()?;
    store.raw_connection().execute_batch(RECIPES_SCHEMA_SQL)?;

    for (id, name) in [
        ("ent_household", "Household"),
        ("ent_business", "Business"),
        ("ent_personal", "Personal"),
    ] {
        store.register_entity(id, name, EntityVisibility::Transparent)?;
    }

    store.create_ledger_account(
        "hh_checking",
        "ent_household",
        "Household Checking",
        "USD",
        AccountElement::Asset,
        AccountRole::Standard,
    )?;
    store.create_ledger_account(
        "hh_visa",
        "ent_household",
        "Household Visa",
        "USD",
        AccountElement::Liability,
        AccountRole::Standard,
    )?;
    store.create_ledger_account(
        "hh_opening_eq",
        "ent_household",
        "Opening Equity",
        "USD",
        AccountElement::Equity,
        AccountRole::Standard,
    )?;
    store.create_ledger_account(
        "hh_groceries",
        "ent_household",
        "Groceries",
        "USD",
        AccountElement::Expense,
        AccountRole::Standard,
    )?;
    store.create_ledger_account(
        "hh_due_from_biz",
        "ent_household",
        "Due From Business",
        "USD",
        AccountElement::Asset,
        AccountRole::Standard,
    )?;
    store.create_ledger_account(
        "biz_supplies",
        "ent_business",
        "Business Supplies",
        "USD",
        AccountElement::Expense,
        AccountRole::Standard,
    )?;
    store.create_ledger_account(
        "biz_due_to_hh",
        "ent_business",
        "Due To Household",
        "USD",
        AccountElement::Liability,
        AccountRole::Standard,
    )?;

    // 1. Direct manual entry without any recipe
    store.post_entry(&JournalEntryDraft::new(
        "2026-10-01",
        "Opening checking balance",
        vec![
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(5000_00, "USD")?,
                PostingSide::Debit,
                "",
            ),
            PostingPrimitive::new(
                "hh_opening_eq",
                "ent_household",
                Money::new(5000_00, "USD")?,
                PostingSide::Credit,
                "",
            ),
        ],
    ))?;

    // 2. AT-001 Split purchase ($150 on Household Visa -> $100 Household + $50 Business)
    let output = RecipeCompiler::split_purchase_across_entities(
        "2026-10-31",
        "ent_household",
        "hh_visa",
        AccountElement::Liability,
        "USD",
        &[
            SplitAllocation {
                target_entity_id: "ent_household",
                expense_account_id: "hh_groceries",
                amount_minor: 100_00,
                funding_due_from_account_id: None,
                target_due_to_account_id: None,
            },
            SplitAllocation {
                target_entity_id: "ent_business",
                expense_account_id: "biz_supplies",
                amount_minor: 50_00,
                funding_due_from_account_id: Some("hh_due_from_biz"),
                target_due_to_account_id: Some("biz_due_to_hh"),
            },
        ],
        "Costco split purchase",
    )?;
    RecipeCompiler::post_recipe(&mut store, &output)?;

    println!(
        "{:<16} {:<6} {:>12} {:>12} {:>12} {:>12} {:>8}",
        "ENTITY", "CURR", "ASSETS", "LIABILITIES", "TOTAL_EQUITY", "DISCREPANCY", "BALANCED"
    );
    for entity_id in ["ent_household", "ent_business"] {
        let eq = store.verify_accounting_equation(entity_id, "USD")?;
        println!(
            "{:<16} {:<6} {:>12.2} {:>12.2} {:>12.2} {:>12.2} {:>8}",
            eq.entity_id,
            eq.currency,
            eq.assets_minor as f64 / 100.0,
            eq.liabilities_minor as f64 / 100.0,
            eq.total_equity_minor as f64 / 100.0,
            eq.equation_discrepancy_minor as f64 / 100.0,
            eq.is_balanced()
        );
    }

    Ok(())
}
