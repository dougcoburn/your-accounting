//! Comprehensive Application Cookbook & Readback Demo (`accounting-cli`)
//!
//! Demonstrates how an application uses the `LedgerStore` Port across 8 varied
//! real-world scenarios, followed by specific entry readbacks, date-filtered
//! account registers, forensic fixup lineage, and aggregate equation summaries.

use accounting_kernel::{
    AccountElement, AccountRole, CommittedJournalEntry, EntityVisibility, EntryFilter,
    JournalEntryDraft, LedgerStore, Money, PostingPrimitive, PostingSide, SqliteLedgerAdapter,
};
use accounting_recipes::{RecipeCompiler, SplitAllocation, RECIPES_SCHEMA_SQL};

fn fmt_minor(minor: i64, currency: &str) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.abs();
    let sym = match currency {
        "USD" => "$",
        "EUR" => "€",
        _ => "",
    };
    format!("{sign}{sym}{}.{:02} {currency}", abs / 100, abs % 100)
}

fn print_entry(entry: &CommittedJournalEntry) {
    println!(
        "  Entry [{}] | Date: {} (Eff: {}) | Status: {:?} | Replaces: {:?}",
        entry.id, entry.posting_date, entry.effective_date, entry.status, entry.replaces_entry_id
    );
    println!("  Description: {}", entry.description);
    for p in &entry.postings {
        println!(
            "    - [{:<6}] {:<24} ({:<13}) {:>14} | {}",
            p.side.as_str().to_uppercase(),
            p.ledger_account_id,
            p.entity_id,
            fmt_minor(p.money.amount_minor(), p.money.currency().as_str()),
            p.memo
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = SqliteLedgerAdapter::in_memory()?;
    store.raw_connection().execute_batch(RECIPES_SCHEMA_SQL)?;

    // =========================================================================
    // 0. REGISTER ENTITIES & CHART OF LEDGER ACCOUNTS
    // =========================================================================
    for (id, name, vis) in [
        ("ent_household", "Household", EntityVisibility::Transparent),
        (
            "ent_business",
            "Consulting LLC",
            EntityVisibility::Transparent,
        ),
        ("ent_personal", "Personal", EntityVisibility::Transparent),
        (
            "ent_roommate",
            "Roommate (Opaque)",
            EntityVisibility::Opaque,
        ),
    ] {
        store.register_entity(id, name, vis)?;
    }

    // Household USD & EUR accounts
    for (id, ent, name, curr, elem, role) in [
        (
            "hh_checking",
            "ent_household",
            "Household Checking",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        ),
        (
            "hh_savings",
            "ent_household",
            "Household Savings",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        ),
        (
            "hh_hsa",
            "ent_household",
            "Household HSA",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        ),
        (
            "hh_eur_wallet",
            "ent_household",
            "Euro Travel Wallet",
            "EUR",
            AccountElement::Asset,
            AccountRole::Standard,
        ),
        (
            "hh_visa",
            "ent_household",
            "Household Visa Card",
            "USD",
            AccountElement::Liability,
            AccountRole::Standard,
        ),
        (
            "hh_opening_eq",
            "ent_household",
            "Opening Equity (USD)",
            "USD",
            AccountElement::Equity,
            AccountRole::Standard,
        ),
        (
            "hh_opening_eq_eur",
            "ent_household",
            "Opening Equity (EUR)",
            "EUR",
            AccountElement::Equity,
            AccountRole::Standard,
        ),
        (
            "hh_salary_rev",
            "ent_household",
            "Salary Revenue",
            "USD",
            AccountElement::Revenue,
            AccountRole::Standard,
        ),
        (
            "hh_hsa_match_rev",
            "ent_household",
            "Employer HSA Match",
            "USD",
            AccountElement::Revenue,
            AccountRole::Standard,
        ),
        (
            "hh_tax_exp",
            "ent_household",
            "Income Tax Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "hh_groceries",
            "ent_household",
            "Groceries Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "hh_insurance",
            "ent_household",
            "Household Insurance",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "hh_electronics",
            "ent_household",
            "Electronics Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "hh_purchase_discounts",
            "ent_household",
            "Purchase Discounts (Contra)",
            "USD",
            AccountElement::Expense,
            AccountRole::Contra,
        ),
        (
            "hh_eur_dining",
            "ent_household",
            "Euro Dining Expense",
            "EUR",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "hh_gifts_exp",
            "ent_household",
            "Gifts & Forgiveness Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "hh_xfer_clearing",
            "ent_household",
            "Transfer Clearing",
            "USD",
            AccountElement::Asset,
            AccountRole::Clearing,
        ),
        (
            "hh_payroll_clearing",
            "ent_household",
            "Payroll Clearing",
            "USD",
            AccountElement::Asset,
            AccountRole::Clearing,
        ),
        (
            "hh_due_from_biz",
            "ent_household",
            "Due From Consulting LLC",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        ),
        (
            "hh_due_from_per",
            "ent_household",
            "Due From Personal",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        ),
        (
            "hh_due_from_roommate",
            "ent_household",
            "Receivable From Roommate",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        ),
        // Business USD accounts
        (
            "biz_insurance",
            "ent_business",
            "Business Liability Insurance",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "biz_due_to_hh",
            "ent_business",
            "Due To Household",
            "USD",
            AccountElement::Liability,
            AccountRole::Standard,
        ),
        // Personal USD accounts
        (
            "per_insurance",
            "ent_personal",
            "Personal Auto Insurance",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        ),
        (
            "per_due_to_hh",
            "ent_personal",
            "Due To Household",
            "USD",
            AccountElement::Liability,
            AccountRole::Standard,
        ),
    ] {
        store.create_ledger_account(id, ent, name, curr, elem, role)?;
    }

    // =========================================================================
    // EXAMPLE 1: OPENING BALANCES (USD & EUR)
    // =========================================================================
    store.post_entry(&JournalEntryDraft::new(
        "2026-10-01",
        "Initial USD & EUR opening balances",
        vec![
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(10000_00, "USD")?,
                PostingSide::Debit,
                "Opening checking",
            ),
            PostingPrimitive::new(
                "hh_savings",
                "ent_household",
                Money::new(5000_00, "USD")?,
                PostingSide::Debit,
                "Opening savings",
            ),
            PostingPrimitive::new(
                "hh_opening_eq",
                "ent_household",
                Money::new(15000_00, "USD")?,
                PostingSide::Credit,
                "Opening USD equity",
            ),
            PostingPrimitive::new(
                "hh_eur_wallet",
                "ent_household",
                Money::new(2000_00, "EUR")?,
                PostingSide::Debit,
                "Opening EUR wallet",
            ),
            PostingPrimitive::new(
                "hh_opening_eq_eur",
                "ent_household",
                Money::new(2000_00, "EUR")?,
                PostingSide::Credit,
                "Opening EUR equity",
            ),
        ],
    ))?;

    // =========================================================================
    // EXAMPLE 2: PURCHASE WITH PROMOTIONAL DISCOUNT (CONTRA-EXPENSE) & EUR SPEND
    // =========================================================================
    let promo_entry_id = store.post_entry(&JournalEntryDraft::new(
        "2026-10-04",
        "Laptop purchase ($1,200 gross less $150 instant rebate)",
        vec![
            PostingPrimitive::new(
                "hh_electronics",
                "ent_household",
                Money::new(1200_00, "USD")?,
                PostingSide::Debit,
                "Gross laptop price",
            ),
            PostingPrimitive::new(
                "hh_purchase_discounts",
                "ent_household",
                Money::new(150_00, "USD")?,
                PostingSide::Credit,
                "Instant promotional rebate",
            ),
            PostingPrimitive::new(
                "hh_visa",
                "ent_household",
                Money::new(1050_00, "USD")?,
                PostingSide::Credit,
                "Net Visa charge",
            ),
        ],
    ))?;

    store.post_entry(&JournalEntryDraft::new(
        "2026-10-05",
        "Cafe lunch in Paris",
        vec![
            PostingPrimitive::new(
                "hh_eur_dining",
                "ent_household",
                Money::new(45_00, "EUR")?,
                PostingSide::Debit,
                "Bistro lunch",
            ),
            PostingPrimitive::new(
                "hh_eur_wallet",
                "ent_household",
                Money::new(45_00, "EUR")?,
                PostingSide::Credit,
                "Paid from EUR wallet",
            ),
        ],
    ))?;

    // =========================================================================
    // EXAMPLE 3: 3-WAY CROSS-ENTITY SPLIT PURCHASE VIA RECIPE COMPILER
    // =========================================================================
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
    let execution_id = unit.next_deterministic_id("rx")?;
    unit.with_connection(|conn| {
        RecipeCompiler::record_execution(
            conn,
            &execution_id,
            &split_recipe.execution,
            &split_entry_id,
        )
    })?;
    unit.commit()?;

    // =========================================================================
    // EXAMPLE 4: MULTI-DATE BANK TRANSFER SETTLEMENT GROUP
    // =========================================================================
    let xfer_out_clear = PostingPrimitive::new(
        "hh_xfer_clearing",
        "ent_household",
        Money::new(1000_00, "USD")?,
        PostingSide::Debit,
        "ACH transfer initiated",
    );
    let revision = store.revision()?;
    let mut unit = store.begin_transaction(revision)?;
    let xfer_out_entry = unit.post_entry_with_ids(&JournalEntryDraft::new(
        "2026-10-10",
        "ACH Transfer Out of Checking",
        vec![
            xfer_out_clear,
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(1000_00, "USD")?,
                PostingSide::Credit,
                "Checking withdrawal",
            ),
        ],
    ))?;
    let xfer_out_clear_id = xfer_out_entry.postings[0].id.clone();

    let xfer_in_clear = PostingPrimitive::new(
        "hh_xfer_clearing",
        "ent_household",
        Money::new(1000_00, "USD")?,
        PostingSide::Credit,
        "ACH transfer settled",
    );
    let xfer_in_entry = unit.post_entry_with_ids(&JournalEntryDraft::new(
        "2026-10-12",
        "ACH Transfer Deposit into Savings",
        vec![
            PostingPrimitive::new(
                "hh_savings",
                "ent_household",
                Money::new(1000_00, "USD")?,
                PostingSide::Debit,
                "Savings deposit",
            ),
            xfer_in_clear,
        ],
    ))?;
    let xfer_in_clear_id = xfer_in_entry.postings[1].id.clone();

    unit.settle_group(
        "sg_ach_oct",
        "Oct 10-12 ACH Transfer ($1,000)",
        &[&xfer_out_clear_id, &xfer_in_clear_id],
    )?;
    unit.commit()?;

    // =========================================================================
    // EXAMPLE 5: PAYROLL GROSS-TO-NET + HSA SETTLEMENT GROUP
    // =========================================================================
    let pay_stub_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(3450_00, "USD")?,
        PostingSide::Debit,
        "Expected net + HSA deposits",
    );
    let revision = store.revision()?;
    let mut unit = store.begin_transaction(revision)?;
    let pay_stub_entry = unit.post_entry_with_ids(&JournalEntryDraft::new(
        "2026-10-15",
        "Oct 15 Paystub ($4,000 salary + $250 HSA match - $800 tax)",
        vec![
            pay_stub_clear,
            PostingPrimitive::new(
                "hh_tax_exp",
                "ent_household",
                Money::new(800_00, "USD")?,
                PostingSide::Debit,
                "Withheld income taxes",
            ),
            PostingPrimitive::new(
                "hh_salary_rev",
                "ent_household",
                Money::new(4000_00, "USD")?,
                PostingSide::Credit,
                "Gross salary",
            ),
            PostingPrimitive::new(
                "hh_hsa_match_rev",
                "ent_household",
                Money::new(250_00, "USD")?,
                PostingSide::Credit,
                "Employer HSA match",
            ),
        ],
    ))?;
    let pay_stub_clear_id = pay_stub_entry.postings[0].id.clone();

    let pay_net_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(3200_00, "USD")?,
        PostingSide::Credit,
        "Clear net pay",
    );
    let pay_net_entry = unit.post_entry_with_ids(&JournalEntryDraft::new(
        "2026-10-15",
        "Oct 15 Payroll Direct Deposit",
        vec![
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(3200_00, "USD")?,
                PostingSide::Debit,
                "Direct deposit",
            ),
            pay_net_clear,
        ],
    ))?;
    let pay_net_clear_id = pay_net_entry.postings[1].id.clone();

    let pay_hsa_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(250_00, "USD")?,
        PostingSide::Credit,
        "Clear HSA deposit",
    );
    let pay_hsa_entry = unit.post_entry_with_ids(&JournalEntryDraft::new(
        "2026-10-16",
        "Oct 16 Employer HSA Deposit",
        vec![
            PostingPrimitive::new(
                "hh_hsa",
                "ent_household",
                Money::new(250_00, "USD")?,
                PostingSide::Debit,
                "HSA contribution",
            ),
            pay_hsa_clear,
        ],
    ))?;
    let pay_hsa_clear_id = pay_hsa_entry.postings[1].id.clone();

    unit.settle_group(
        "sg_payroll_oct15",
        "Oct 15 Payroll Settlement",
        &[&pay_stub_clear_id, &pay_net_clear_id, &pay_hsa_clear_id],
    )?;
    unit.commit()?;

    // =========================================================================
    // EXAMPLE 6: OPAQUE PARTY ADVANCE, PARTIAL REIMBURSEMENT & FORGIVENESS
    // =========================================================================
    store.post_entry(&JournalEntryDraft::new(
        "2026-10-17",
        "Cover roommate share of utility deposit ($300)",
        vec![
            PostingPrimitive::new(
                "hh_due_from_roommate",
                "ent_household",
                Money::new(300_00, "USD")?,
                PostingSide::Debit,
                "Receivable from Roommate",
            ),
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(300_00, "USD")?,
                PostingSide::Credit,
                "Paid from checking",
            ),
        ],
    ))?;

    store.post_entry(&JournalEntryDraft::new(
        "2026-10-20",
        "Venmo partial repayment from roommate ($180)",
        vec![
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(180_00, "USD")?,
                PostingSide::Debit,
                "Venmo deposit",
            ),
            PostingPrimitive::new(
                "hh_due_from_roommate",
                "ent_household",
                Money::new(180_00, "USD")?,
                PostingSide::Credit,
                "Reduce roommate receivable",
            ),
        ],
    ))?;

    store.post_entry(&JournalEntryDraft::new(
        "2026-10-22",
        "Forgive remaining $120 roommate balance as birthday gift",
        vec![
            PostingPrimitive::new(
                "hh_gifts_exp",
                "ent_household",
                Money::new(120_00, "USD")?,
                PostingSide::Debit,
                "Birthday gift forgiveness",
            ),
            PostingPrimitive::new(
                "hh_due_from_roommate",
                "ent_household",
                Money::new(120_00, "USD")?,
                PostingSide::Credit,
                "Close receivable",
            ),
        ],
    ))?;

    // =========================================================================
    // EXAMPLE 7: MISTAKE & PAINLESS FIXUP LINEAGE (`ACTIVE` vs `SUPERSEDED`)
    // =========================================================================
    let mistaken_id = store.post_entry(&JournalEntryDraft::new(
        "2026-10-18",
        "Weekly groceries (TYPO: entered $950.00 instead of $95.00)",
        vec![
            PostingPrimitive::new(
                "hh_groceries",
                "ent_household",
                Money::new(950_00, "USD")?,
                PostingSide::Debit,
                "Typo amount",
            ),
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(950_00, "USD")?,
                PostingSide::Credit,
                "Checking debit",
            ),
        ],
    ))?;

    let corrected_id = store.post_entry(&JournalEntryDraft::with_replacement(
        "2026-10-18",
        "Weekly groceries (Corrected to $95.00)",
        &mistaken_id,
        vec![
            PostingPrimitive::new(
                "hh_groceries",
                "ent_household",
                Money::new(95_00, "USD")?,
                PostingSide::Debit,
                "Corrected grocery receipt",
            ),
            PostingPrimitive::new(
                "hh_checking",
                "ent_household",
                Money::new(95_00, "USD")?,
                PostingSide::Credit,
                "Checking debit",
            ),
        ],
    ))?;

    // =========================================================================
    // READBACK 1: SPECIFIC SINGLE-ENTRY READS (`get_entry`)
    // =========================================================================
    println!("================================================================================");
    println!("1. SPECIFIC ENTRY READBACK (`get_entry`)");
    println!("================================================================================");
    let promo_entry = store
        .get_entry(&promo_entry_id)?
        .expect("promo entry exists");
    print_entry(&promo_entry);
    println!();
    let split_entry = store
        .get_entry(&split_entry_id)?
        .expect("split entry exists");
    print_entry(&split_entry);

    // =========================================================================
    // READBACK 2: DATE-FILTERED ACTIVE REGISTER (`list_active_entries_for_account`)
    // =========================================================================
    println!("\n================================================================================");
    println!("2. DATE-FILTERED ACTIVE REGISTER (`hh_checking` from 2026-10-10 to 2026-10-20)");
    println!("   Notice the $950 typo on 2026-10-18 is hidden; only the $95 fixup appears!");
    println!("================================================================================");
    let register_filter = EntryFilter::date_range("2026-10-10", "2026-10-20");
    let checking_entries =
        store.list_active_entries_for_account("hh_checking", &register_filter)?;
    for entry in &checking_entries {
        print_entry(entry);
        println!(
            "  ------------------------------------------------------------------------------"
        );
    }

    // =========================================================================
    // READBACK 3: FORENSIC AUDIT LINEAGE (`get_entry_lineage`)
    // =========================================================================
    println!("\n================================================================================");
    println!(
        "3. FORENSIC AUDIT LINEAGE (`get_entry_lineage` for corrected entry {})",
        corrected_id
    );
    println!("================================================================================");
    let lineage = store.get_entry_lineage(&corrected_id)?;
    for (idx, item) in lineage.iter().enumerate() {
        println!("  Revision #{}:", idx + 1);
        print_entry(item);
    }

    // =========================================================================
    // READBACK 4: PER-ACCOUNT NORMAL BALANCES (`list_accounts` + `get_account_balance`)
    // =========================================================================
    println!("\n================================================================================");
    println!("4. PER-ACCOUNT NORMAL BALANCES BY ENTITY");
    println!("================================================================================");
    println!(
        "{:<15} {:<24} {:<10} {:<10} {:>16}",
        "ENTITY", "ACCOUNT_ID", "ELEMENT", "ROLE", "NORMAL_BALANCE"
    );
    for entity_id in ["ent_household", "ent_business", "ent_personal"] {
        for acct in store.list_accounts(entity_id)? {
            let bal = store.get_account_balance(&acct.id)?;
            if bal != 0 || acct.role == AccountRole::Clearing {
                println!(
                    "{:<15} {:<24} {:<10} {:<10} {:>16}",
                    acct.entity_id,
                    acct.id,
                    acct.element.as_str(),
                    acct.role.as_str(),
                    fmt_minor(bal, acct.currency.as_str())
                );
            }
        }
    }

    // =========================================================================
    // READBACK 5: AGGREGATE EQUATION SUMMARIES (`summarize_accounting_equation`)
    // =========================================================================
    println!("\n================================================================================");
    println!("5. AGGREGATE ACCOUNTING EQUATION SUMMARIES (`summarize_accounting_equation`)");
    println!("   Assets == Liabilities + PaidInEquity + (Revenue - Expenses)");
    println!("================================================================================");
    println!(
        "{:<15} {:<5} {:>12} {:>12} {:>12} {:>12} {:>12} {:>12} {:>8}",
        "ENTITY",
        "CURR",
        "ASSETS",
        "LIABILITIES",
        "PAID_EQUITY",
        "REVENUE",
        "EXPENSES",
        "TOTAL_EQ",
        "BALANCED"
    );
    for (entity_id, curr) in [
        ("ent_household", "USD"),
        ("ent_household", "EUR"),
        ("ent_business", "USD"),
        ("ent_personal", "USD"),
    ] {
        let eq = store.summarize_accounting_equation(entity_id, curr)?;
        println!(
            "{:<15} {:<5} {:>12.2} {:>12.2} {:>12.2} {:>12.2} {:>12.2} {:>12.2} {:>8}",
            eq.entity_id,
            eq.currency,
            eq.assets_minor as f64 / 100.0,
            eq.liabilities_minor as f64 / 100.0,
            eq.equity_paid_in_minor as f64 / 100.0,
            eq.revenue_minor as f64 / 100.0,
            eq.expenses_minor as f64 / 100.0,
            eq.total_equity_minor as f64 / 100.0,
            eq.is_balanced()
        );
    }

    Ok(())
}
