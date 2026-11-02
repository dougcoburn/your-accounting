//! Executable Acceptance Test Suite (AT-001..030 + Read Symmetry, Date Filters, and Fixup Lineage)

use accounting_kernel::SqliteLedgerAdapter;
use accounting_kernel::{
    AccountElement, AccountRole, EntityVisibility, EntryFilter, EntryStatus, JournalEntryDraft,
    KernelError, LedgerStore, Money, PostingPrimitive, PostingSide,
};
use accounting_recipes::{RecipeCompiler, SplitAllocation, RECIPES_SCHEMA_SQL};

fn setup_seeded_store() -> SqliteLedgerAdapter {
    let mut adapter = SqliteLedgerAdapter::in_memory().expect("in-memory SQLite adapter");

    adapter
        .raw_connection()
        .execute_batch(RECIPES_SCHEMA_SQL)
        .unwrap();

    adapter
        .register_entity("ent_household", "Household", EntityVisibility::Transparent)
        .unwrap();
    adapter
        .register_entity("ent_business", "Business", EntityVisibility::Transparent)
        .unwrap();
    adapter
        .register_entity("ent_personal", "Personal", EntityVisibility::Transparent)
        .unwrap();
    adapter
        .register_entity(
            "ent_child_opaque",
            "Child (Opaque)",
            EntityVisibility::Opaque,
        )
        .unwrap();

    adapter
        .create_ledger_account(
            "hh_checking",
            "ent_household",
            "Household Checking",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_savings",
            "ent_household",
            "Household Savings",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_hsa",
            "ent_household",
            "Household HSA",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_visa",
            "ent_household",
            "Household Visa Card",
            "USD",
            AccountElement::Liability,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_opening_eq",
            "ent_household",
            "Household Opening Equity",
            "USD",
            AccountElement::Equity,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_salary_rev",
            "ent_household",
            "Salary Revenue",
            "USD",
            AccountElement::Revenue,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_employer_match_rev",
            "ent_household",
            "Employer HSA Match",
            "USD",
            AccountElement::Revenue,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_tax_exp",
            "ent_household",
            "Income Tax Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_groceries",
            "ent_household",
            "Household Groceries",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_gifts_exp",
            "ent_household",
            "Household Gifts Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_transfer_clearing",
            "ent_household",
            "Household Transfer Clearing",
            "USD",
            AccountElement::Asset,
            AccountRole::Clearing,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_payroll_clearing",
            "ent_household",
            "Household Payroll Clearing",
            "USD",
            AccountElement::Asset,
            AccountRole::Clearing,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_due_from_biz",
            "ent_household",
            "Due From Business",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_due_from_personal",
            "ent_household",
            "Due From Personal",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "hh_due_from_child",
            "ent_household",
            "Receivable From Child (Opaque)",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();

    adapter
        .create_ledger_account(
            "biz_supplies",
            "ent_business",
            "Business Supplies",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "biz_due_to_hh",
            "ent_business",
            "Due To Household",
            "USD",
            AccountElement::Liability,
            AccountRole::Standard,
        )
        .unwrap();

    adapter
        .create_ledger_account(
            "per_gift_card",
            "ent_personal",
            "Personal Store Gift Card",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();
    adapter
        .create_ledger_account(
            "per_due_to_hh",
            "ent_personal",
            "Due To Household",
            "USD",
            AccountElement::Liability,
            AccountRole::Standard,
        )
        .unwrap();

    adapter
}

#[test]
fn test_at001_split_purchase_across_entities_preserves_equation() {
    let mut store = setup_seeded_store();

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
        "Costco split run",
    )
    .unwrap();

    let revision = store.revision().unwrap();
    let mut unit = store.begin_transaction(revision).unwrap();
    let entry_id = RecipeCompiler::post_recipe(&mut unit, &output).unwrap();
    let execution_id = unit.next_deterministic_id("rx").unwrap();
    unit.with_connection(|conn| {
        RecipeCompiler::record_execution(conn, &execution_id, &output.execution, &entry_id)
    })
    .unwrap();
    assert_eq!(execution_id, "rx_1");
    unit.commit().unwrap();

    let hh_eq = store
        .summarize_accounting_equation("ent_household", "USD")
        .unwrap();
    let biz_eq = store
        .summarize_accounting_equation("ent_business", "USD")
        .unwrap();

    assert_eq!(hh_eq.assets_minor, 50_00);
    assert_eq!(hh_eq.liabilities_minor, 150_00);
    assert_eq!(hh_eq.expenses_minor, 100_00);
    assert_eq!(hh_eq.total_equity_minor, -100_00);
    assert!(hh_eq.is_balanced());

    assert_eq!(biz_eq.assets_minor, 0);
    assert_eq!(biz_eq.liabilities_minor, 50_00);
    assert_eq!(biz_eq.expenses_minor, 50_00);
    assert_eq!(biz_eq.total_equity_minor, -50_00);
    assert!(biz_eq.is_balanced());
}

#[test]
fn test_read_symmetry_and_date_filtering() {
    let mut store = setup_seeded_store();

    // 1. Verify get_account and list_accounts symmetry
    let acct = store
        .get_account("hh_checking")
        .unwrap()
        .expect("account exists");
    assert_eq!(acct.name, "Household Checking");
    assert_eq!(acct.element, AccountElement::Asset);
    assert_eq!(acct.currency.as_str(), "USD");

    let hh_accounts = store.list_accounts("ent_household").unwrap();
    assert!(hh_accounts.len() >= 10);

    // 2. Post three transactions across different dates
    let e1 = store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-01",
            "Oct 1 Opening",
            vec![
                PostingPrimitive::new(
                    "hh_checking",
                    "ent_household",
                    Money::new(1000_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_opening_eq",
                    "ent_household",
                    Money::new(1000_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    let e2 = store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-15",
            "Oct 15 Grocery Run",
            vec![
                PostingPrimitive::new(
                    "hh_groceries",
                    "ent_household",
                    Money::new(150_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_checking",
                    "ent_household",
                    Money::new(150_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    let _e3 = store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-25",
            "Oct 25 Grocery Run 2",
            vec![
                PostingPrimitive::new(
                    "hh_groceries",
                    "ent_household",
                    Money::new(80_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_checking",
                    "ent_household",
                    Money::new(80_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    // Symmetrical get_entry
    let fetched_e2 = store.get_entry(&e2).unwrap().expect("entry exists");
    assert_eq!(fetched_e2.description, "Oct 15 Grocery Run");
    assert_eq!(fetched_e2.status, EntryStatus::Active);
    assert_eq!(fetched_e2.postings.len(), 2);

    // Filter by date range: Oct 10 to Oct 20 should ONLY return e2!
    let filter_mid = EntryFilter::date_range("2026-10-10", "2026-10-20");
    let mid_entries = store
        .list_active_entries_for_account("hh_checking", &filter_mid)
        .unwrap();
    assert_eq!(mid_entries.len(), 1);
    assert_eq!(mid_entries[0].id, e2);

    // Filter all with limit 2
    let filter_limit = EntryFilter::new().with_pagination(2, 0);
    let limited = store
        .list_active_entries_for_account("hh_checking", &filter_limit)
        .unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].id, e1);
    assert_eq!(limited[1].id, e2);
}

#[test]
fn test_immutable_fixup_lineage_and_transparent_active_view() {
    let mut store = setup_seeded_store();

    // 1. Initial purchase with mistake: $150 entered as $1500
    let original_id = store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-12",
            "Original mistaken purchase",
            vec![
                PostingPrimitive::new(
                    "hh_groceries",
                    "ent_household",
                    Money::new(1500_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_visa",
                    "ent_household",
                    Money::new(1500_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    assert_eq!(store.get_account_balance("hh_groceries").unwrap(), 1500_00);

    // 2. Fixup: Post replacement entry for $150 pointing to original_id
    let fixup_id = store
        .post_entry(&JournalEntryDraft::with_replacement(
            "2026-10-12",
            "Corrected purchase ($150)",
            &original_id,
            vec![
                PostingPrimitive::new(
                    "hh_groceries",
                    "ent_household",
                    Money::new(150_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_visa",
                    "ent_household",
                    Money::new(150_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    // 3. Normal Workflow Transparency:
    // Balance now cleanly shows $150 (not $1500 + reversal noise)
    assert_eq!(store.get_account_balance("hh_groceries").unwrap(), 150_00);
    assert_eq!(store.get_account_balance("hh_visa").unwrap(), 150_00);

    // Active entries query only shows the fixup! Mistake is hidden from day-to-day register:
    let active_entries = store
        .list_active_entries_for_account("hh_groceries", &EntryFilter::new())
        .unwrap();
    assert_eq!(active_entries.len(), 1);
    assert_eq!(active_entries[0].id, fixup_id);
    assert_eq!(active_entries[0].status, EntryStatus::Active);

    // 4. Forensic Audit:
    // Checking the original entry proves it was not erased—it is marked superseded
    let original_entry = store.get_entry(&original_id).unwrap().unwrap();
    assert_eq!(original_entry.status, EntryStatus::Superseded);

    // Lineage traces both the mistake and the fixup in order
    let lineage = store.get_entry_lineage(&fixup_id).unwrap();
    assert_eq!(lineage.len(), 2);
    assert_eq!(lineage[0].id, original_id);
    assert_eq!(lineage[0].status, EntryStatus::Superseded);
    assert_eq!(lineage[1].id, fixup_id);
    assert_eq!(lineage[1].status, EntryStatus::Active);
}

#[test]
fn test_at005_006_007_opaque_receivable_reimbursement_and_forgiveness() {
    let mut store = setup_seeded_store();

    store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-01",
            "Initial checking balance",
            vec![
                PostingPrimitive::new(
                    "hh_checking",
                    "ent_household",
                    Money::new(5000_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_opening_eq",
                    "ent_household",
                    Money::new(5000_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-05",
            "Advance for child school trip",
            vec![
                PostingPrimitive::new(
                    "hh_due_from_child",
                    "ent_household",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_checking",
                    "ent_household",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();
    assert_eq!(
        store.get_account_balance("hh_due_from_child").unwrap(),
        100_00
    );

    store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-12",
            "Partial repayment from child",
            vec![
                PostingPrimitive::new(
                    "hh_checking",
                    "ent_household",
                    Money::new(40_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_due_from_child",
                    "ent_household",
                    Money::new(40_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();
    assert_eq!(
        store.get_account_balance("hh_due_from_child").unwrap(),
        60_00
    );

    store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-20",
            "Forgive remaining $60 as gift",
            vec![
                PostingPrimitive::new(
                    "hh_gifts_exp",
                    "ent_household",
                    Money::new(60_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_due_from_child",
                    "ent_household",
                    Money::new(60_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    assert_eq!(store.get_account_balance("hh_due_from_child").unwrap(), 0);
    let hh_eq = store
        .summarize_accounting_equation("ent_household", "USD")
        .unwrap();
    assert_eq!(hh_eq.assets_minor, 4940_00);
    assert_eq!(hh_eq.total_equity_minor, 4940_00);
    assert!(hh_eq.is_balanced());
}

#[test]
fn test_at003_and_at030_settlement_group_zero_residual_enforcement() {
    let mut store = setup_seeded_store();

    let p_leg1_clear = PostingPrimitive::new(
        "hh_transfer_clearing",
        "ent_household",
        Money::new(500_00, "USD").unwrap(),
        PostingSide::Debit,
        "",
    );
    let p_leg1_bank = PostingPrimitive::new(
        "hh_checking",
        "ent_household",
        Money::new(500_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );

    let posted_leg1 = store
        .post_entry_with_ids(&JournalEntryDraft::new(
            "2026-10-10",
            "Checking withdrawal to savings",
            vec![p_leg1_clear, p_leg1_bank],
        ))
        .unwrap();
    let p_leg1_clear_id = posted_leg1.postings[0].id.clone();

    let err = store
        .settle_group(
            "sg_incomplete",
            "Incomplete transfer",
            &[p_leg1_clear_id.as_str()],
        )
        .unwrap_err();
    assert!(matches!(err, KernelError::NonZeroClearingResidual { .. }));

    let p_leg2_bank = PostingPrimitive::new(
        "hh_savings",
        "ent_household",
        Money::new(500_00, "USD").unwrap(),
        PostingSide::Debit,
        "",
    );
    let p_leg2_clear = PostingPrimitive::new(
        "hh_transfer_clearing",
        "ent_household",
        Money::new(500_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );

    let posted_leg2 = store
        .post_entry_with_ids(&JournalEntryDraft::new(
            "2026-10-12",
            "Savings deposit from checking",
            vec![p_leg2_bank, p_leg2_clear],
        ))
        .unwrap();
    let p_leg2_clear_id = posted_leg2.postings[1].id.clone();

    store
        .settle_group(
            "sg_complete",
            "Matched $500 transfer",
            &[p_leg1_clear_id.as_str(), p_leg2_clear_id.as_str()],
        )
        .unwrap();
    assert_eq!(
        store.get_account_balance("hh_transfer_clearing").unwrap(),
        0
    );
    store
        .summarize_accounting_equation("ent_household", "USD")
        .unwrap();
}

#[test]
fn test_at016_cross_entity_refund_to_different_entity_account() {
    let mut store = setup_seeded_store();

    store
        .post_entry(&JournalEntryDraft::new(
            "2026-09-15",
            "Original Household purchase",
            vec![
                PostingPrimitive::new(
                    "hh_groceries",
                    "ent_household",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_visa",
                    "ent_household",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-02",
            "Refund to Personal Gift Card",
            vec![
                PostingPrimitive::new(
                    "hh_due_from_personal",
                    "ent_household",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_groceries",
                    "ent_household",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
                PostingPrimitive::new(
                    "per_gift_card",
                    "ent_personal",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "per_due_to_hh",
                    "ent_personal",
                    Money::new(100_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();

    let hh_eq = store
        .summarize_accounting_equation("ent_household", "USD")
        .unwrap();
    let per_eq = store
        .summarize_accounting_equation("ent_personal", "USD")
        .unwrap();

    assert_eq!(hh_eq.expenses_minor, 0);
    assert_eq!(hh_eq.assets_minor, 100_00);
    assert_eq!(hh_eq.liabilities_minor, 100_00);
    assert_eq!(per_eq.assets_minor, 100_00);
    assert_eq!(per_eq.liabilities_minor, 100_00);
    assert!(hh_eq.is_balanced());
    assert!(per_eq.is_balanced());
}

#[test]
fn test_at019_and_at020_payroll_gross_to_net_clearing() {
    let mut store = setup_seeded_store();

    let p_stub_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(2700_00, "USD").unwrap(),
        PostingSide::Debit,
        "",
    );

    let posted_stub = store
        .post_entry_with_ids(&JournalEntryDraft::new(
            "2026-10-15",
            "Oct 15 Paystub",
            vec![
                p_stub_clear,
                PostingPrimitive::new(
                    "hh_tax_exp",
                    "ent_household",
                    Money::new(500_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_salary_rev",
                    "ent_household",
                    Money::new(3000_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
                PostingPrimitive::new(
                    "hh_employer_match_rev",
                    "ent_household",
                    Money::new(200_00, "USD").unwrap(),
                    PostingSide::Credit,
                    "",
                ),
            ],
        ))
        .unwrap();
    let p_stub_clear_id = posted_stub.postings[0].id.clone();

    let p_net_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(2500_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );

    let posted_net = store
        .post_entry_with_ids(&JournalEntryDraft::new(
            "2026-10-15",
            "Net pay direct deposit",
            vec![
                PostingPrimitive::new(
                    "hh_checking",
                    "ent_household",
                    Money::new(2500_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                p_net_clear,
            ],
        ))
        .unwrap();
    let p_net_clear_id = posted_net.postings[1].id.clone();

    let err = store
        .settle_group(
            "sg_payroll_incomplete",
            "Missing HSA leg",
            &[p_stub_clear_id.as_str(), p_net_clear_id.as_str()],
        )
        .unwrap_err();
    assert!(matches!(err, KernelError::NonZeroClearingResidual { .. }));

    let p_hsa_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(200_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );

    let posted_hsa = store
        .post_entry_with_ids(&JournalEntryDraft::new(
            "2026-10-17",
            "HSA employer contribution deposit",
            vec![
                PostingPrimitive::new(
                    "hh_hsa",
                    "ent_household",
                    Money::new(200_00, "USD").unwrap(),
                    PostingSide::Debit,
                    "",
                ),
                p_hsa_clear,
            ],
        ))
        .unwrap();
    let p_hsa_clear_id = posted_hsa.postings[1].id.clone();

    store
        .settle_group(
            "sg_payroll_complete",
            "Oct 15 Payroll Settlement",
            &[
                p_stub_clear_id.as_str(),
                p_net_clear_id.as_str(),
                p_hsa_clear_id.as_str(),
            ],
        )
        .unwrap();

    assert_eq!(store.get_account_balance("hh_payroll_clearing").unwrap(), 0);
    let hh_eq = store
        .summarize_accounting_equation("ent_household", "USD")
        .unwrap();
    assert_eq!(hh_eq.assets_minor, 2700_00);
    assert_eq!(hh_eq.revenue_minor, 3200_00);
    assert_eq!(hh_eq.expenses_minor, 500_00);
    assert_eq!(hh_eq.total_equity_minor, 2700_00);
    assert!(hh_eq.is_balanced());
}

#[test]
fn test_contract_error_variants_match_spec() {
    let mut store = setup_seeded_store();

    // 1. AccountOwnerNotTransparent when creating account on Opaque entity
    let err = store
        .create_ledger_account(
            "child_acct",
            "ent_child_opaque",
            "Invalid Child Account",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap_err();
    assert_eq!(
        err,
        KernelError::AccountOwnerNotTransparent {
            entity_id: "ent_child_opaque".to_string()
        }
    );

    // 2. InvalidClearingElement when creating Clearing account on Expense
    let err = store
        .create_ledger_account(
            "bad_clearing",
            "ent_household",
            "Invalid Expense Clearing",
            "USD",
            AccountElement::Expense,
            AccountRole::Clearing,
        )
        .unwrap_err();
    assert_eq!(
        err,
        KernelError::InvalidClearingElement {
            account_id: "bad_clearing".to_string(),
            element: AccountElement::Expense,
        }
    );

    // 3. PostingAccountMismatch when posting entity/currency differs from LedgerAccount
    let p1 = PostingPrimitive::new(
        "hh_checking",
        "ent_business", // wrong entity for hh_checking!
        Money::new(100_00, "USD").unwrap(),
        PostingSide::Debit,
        "",
    );
    let p1_id = p1.id.clone();
    let p2 = PostingPrimitive::new(
        "biz_due_to_hh",
        "ent_business",
        Money::new(100_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );
    let err = store
        .post_entry(&JournalEntryDraft::new(
            "2026-10-31",
            "Mismatch",
            vec![p1, p2],
        ))
        .unwrap_err();
    assert_eq!(
        err,
        KernelError::PostingAccountMismatch {
            posting_id: p1_id,
            ledger_account_id: "hh_checking".to_string(),
            expected_entity_id: "ent_household".to_string(),
            actual_entity_id: "ent_business".to_string(),
            expected_currency: "USD".to_string(),
            actual_currency: "USD".to_string(),
        }
    );
}
