//! Executable Acceptance Test Suite (AT-001, AT-003, AT-005..007, AT-013, AT-016, AT-019..020, AT-030)

use accounting_kernel::{
    AccountElement, AccountRole, AccountingKernel, EntityVisibility, JournalEntryDraft,
    KernelError, Money, PostingPrimitive, PostingSide,
};
use accounting_scenarios::{ScenarioCompiler, SplitAllocation};

fn setup_seeded_kernel() -> AccountingKernel {
    let mut kernel = AccountingKernel::in_memory().expect("in-memory SQLite kernel");

    // Register transparent entities (Household, Business, Personal) and an opaque entity (Child)
    kernel
        .register_entity("ent_household", "Household", EntityVisibility::Transparent)
        .unwrap();
    kernel
        .register_entity("ent_business", "Business", EntityVisibility::Transparent)
        .unwrap();
    kernel
        .register_entity("ent_personal", "Personal", EntityVisibility::Transparent)
        .unwrap();
    kernel
        .register_entity("ent_child_opaque", "Child (Opaque)", EntityVisibility::Opaque)
        .unwrap();

    // Household Ledger Accounts
    kernel
        .create_ledger_account(
            "hh_checking",
            "ent_household",
            "Household Checking",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_savings",
            "ent_household",
            "Household Savings",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_hsa",
            "ent_household",
            "Household HSA",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_visa",
            "ent_household",
            "Household Visa Card",
            "USD",
            AccountElement::Liability,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_opening_eq",
            "ent_household",
            "Household Opening Equity",
            "USD",
            AccountElement::Equity,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_salary_rev",
            "ent_household",
            "Salary Revenue",
            "USD",
            AccountElement::Revenue,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_employer_match_rev",
            "ent_household",
            "Employer HSA Match",
            "USD",
            AccountElement::Revenue,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_tax_exp",
            "ent_household",
            "Income Tax Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_groceries",
            "ent_household",
            "Household Groceries",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_gifts_exp",
            "ent_household",
            "Household Gifts Expense",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_transfer_clearing",
            "ent_household",
            "Household Transfer Clearing",
            "USD",
            AccountElement::Asset,
            AccountRole::Clearing,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_payroll_clearing",
            "ent_household",
            "Household Payroll Clearing",
            "USD",
            AccountElement::Asset,
            AccountRole::Clearing,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_due_from_biz",
            "ent_household",
            "Due From Business",
            "USD",
            AccountElement::Asset,
            AccountRole::InterEntity,
            Some("ent_business"),
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_due_from_personal",
            "ent_household",
            "Due From Personal",
            "USD",
            AccountElement::Asset,
            AccountRole::InterEntity,
            Some("ent_personal"),
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "hh_due_from_child",
            "ent_household",
            "Receivable From Child (Opaque)",
            "USD",
            AccountElement::Asset,
            AccountRole::InterEntity,
            Some("ent_child_opaque"),
        )
        .unwrap();

    // Business Ledger Accounts
    kernel
        .create_ledger_account(
            "biz_supplies",
            "ent_business",
            "Business Supplies",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "biz_due_to_hh",
            "ent_business",
            "Due To Household",
            "USD",
            AccountElement::Liability,
            AccountRole::InterEntity,
            Some("ent_household"),
        )
        .unwrap();

    // Personal Ledger Accounts
    kernel
        .create_ledger_account(
            "per_gift_card",
            "ent_personal",
            "Personal Store Gift Card",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
            None,
        )
        .unwrap();
    kernel
        .create_ledger_account(
            "per_due_to_hh",
            "ent_personal",
            "Due To Household",
            "USD",
            AccountElement::Liability,
            AccountRole::InterEntity,
            Some("ent_household"),
        )
        .unwrap();

    kernel
}

#[test]
fn test_at001_split_purchase_across_entities_preserves_equation() {
    let mut kernel = setup_seeded_kernel();

    let draft = ScenarioCompiler::split_purchase_across_entities(
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

    kernel.post_entry(&draft).unwrap();

    let hh_eq = kernel
        .verify_accounting_equation("ent_household", "USD")
        .unwrap();
    let biz_eq = kernel
        .verify_accounting_equation("ent_business", "USD")
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
fn test_at005_006_007_opaque_receivable_reimbursement_and_forgiveness() {
    let mut kernel = setup_seeded_kernel();

    // 1. Seed $5,000 opening checking balance (AT-010)
    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-01",
            "core:opening_balance:v1",
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

    // 2. AT-005: Household advances $100 for Child
    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-05",
            "core:opaque_advance:v1",
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
        kernel.get_account_balance("hh_due_from_child").unwrap(),
        100_00
    );

    // 3. AT-006: Partial reimbursement of $40
    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-12",
            "core:opaque_reimbursement:v1",
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
        kernel.get_account_balance("hh_due_from_child").unwrap(),
        60_00
    );

    // 4. AT-007: Forgive remaining $60 as gift (new event)
    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-20",
            "core:debt_forgiveness:v1",
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

    assert_eq!(kernel.get_account_balance("hh_due_from_child").unwrap(), 0);
    let hh_eq = kernel
        .verify_accounting_equation("ent_household", "USD")
        .unwrap();
    assert_eq!(hh_eq.assets_minor, 4940_00);
    assert_eq!(hh_eq.total_equity_minor, 4940_00);
    assert!(hh_eq.is_balanced());
}

#[test]
fn test_at003_and_at030_settlement_group_zero_residual_enforcement() {
    let mut kernel = setup_seeded_kernel();

    let p_leg1_clear = PostingPrimitive::new(
        "hh_transfer_clearing",
        "ent_household",
        Money::new(500_00, "USD").unwrap(),
        PostingSide::Debit,
        "",
    );
    let p_leg1_clear_id = p_leg1_clear.id.clone();
    let p_leg1_bank = PostingPrimitive::new(
        "hh_checking",
        "ent_household",
        Money::new(500_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );

    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-10",
            "core:transfer_out:v1",
            "Checking withdrawal to savings",
            vec![p_leg1_clear, p_leg1_bank],
        ))
        .unwrap();

    // Attempting to settle with only leg 1 MUST fail SQLite trigger `trg_settlement_group_seal`
    let err = kernel
        .settle_group(
            "sg_incomplete",
            "Incomplete transfer",
            &[p_leg1_clear_id.as_str()],
        )
        .unwrap_err();
    assert!(matches!(err, KernelError::Storage(_)));

    // Post leg 2 (Savings deposit $500)
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
    let p_leg2_clear_id = p_leg2_clear.id.clone();

    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-12",
            "core:transfer_in:v1",
            "Savings deposit from checking",
            vec![p_leg2_bank, p_leg2_clear],
        ))
        .unwrap();

    // Now settling both clearing postings succeeds with 0 residual
    kernel
        .settle_group(
            "sg_complete",
            "Matched $500 transfer",
            &[p_leg1_clear_id.as_str(), p_leg2_clear_id.as_str()],
        )
        .unwrap();
    assert_eq!(
        kernel.get_account_balance("hh_transfer_clearing").unwrap(),
        0
    );
    kernel
        .verify_accounting_equation("ent_household", "USD")
        .unwrap();
}

#[test]
fn test_at016_cross_entity_refund_to_different_entity_account() {
    let mut kernel = setup_seeded_kernel();

    // Original Household purchase ($100 on Visa)
    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-09-15",
            "core:purchase:v1",
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

    // Refund of $100 lands on Personal Store Gift Card
    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-02",
            "core:cross_entity_refund:v1",
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

    let hh_eq = kernel
        .verify_accounting_equation("ent_household", "USD")
        .unwrap();
    let per_eq = kernel
        .verify_accounting_equation("ent_personal", "USD")
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
    let mut kernel = setup_seeded_kernel();

    let p_stub_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(2700_00, "USD").unwrap(),
        PostingSide::Debit,
        "",
    );
    let p_stub_clear_id = p_stub_clear.id.clone();

    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-15",
            "core:payroll_stub:v1",
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

    let p_net_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(2500_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );
    let p_net_clear_id = p_net_clear.id.clone();

    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-15",
            "core:payroll_net_deposit:v1",
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

    // AT-020: Missing the $200 HSA deposit -> settlement group MUST reject ($200 residual)
    let err = kernel
        .settle_group(
            "sg_payroll_incomplete",
            "Missing HSA leg",
            &[p_stub_clear_id.as_str(), p_net_clear_id.as_str()],
        )
        .unwrap_err();
    assert!(matches!(err, KernelError::Storage(_)));

    // 3. HSA deposit arrives ($200)
    let p_hsa_clear = PostingPrimitive::new(
        "hh_payroll_clearing",
        "ent_household",
        Money::new(200_00, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );
    let p_hsa_clear_id = p_hsa_clear.id.clone();

    kernel
        .post_entry(&JournalEntryDraft::new(
            "2026-10-17",
            "core:payroll_hsa_deposit:v1",
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

    // AT-019: All legs present -> payroll settlement group seals with 0 clearing residual
    kernel
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

    assert_eq!(
        kernel.get_account_balance("hh_payroll_clearing").unwrap(),
        0
    );
    let hh_eq = kernel
        .verify_accounting_equation("ent_household", "USD")
        .unwrap();
    assert_eq!(hh_eq.assets_minor, 2700_00);
    assert_eq!(hh_eq.revenue_minor, 3200_00);
    assert_eq!(hh_eq.expenses_minor, 500_00);
    assert_eq!(hh_eq.total_equity_minor, 2700_00);
    assert!(hh_eq.is_balanced());
}

#[test]
fn test_at013_database_trigger_blocks_unbalanced_or_mutated_postings() {
    let mut kernel = setup_seeded_kernel();
    let conn = kernel.connection_mut();

    conn.execute(
        "INSERT INTO journal_entries (id, posting_date, effective_date, scenario_id, status)
         VALUES ('je_bad', '2026-10-31', '2026-10-31', 'raw_test', 'draft')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO journal_postings
            (id, entry_id, ledger_account_id, entity_id, currency, side, amount_minor)
         VALUES ('p_bad_1', 'je_bad', 'hh_checking', 'ent_household', 'USD', 'debit', 10000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO journal_postings
            (id, entry_id, ledger_account_id, entity_id, currency, side, amount_minor)
         VALUES ('p_bad_2', 'je_bad', 'hh_visa', 'ent_household', 'USD', 'credit', 9900)",
        [],
    )
    .unwrap();

    // Attempting to seal 'je_bad' to 'posted' triggers `trg_journal_entry_seal_balance`
    let res = conn.execute(
        "UPDATE journal_entries SET status = 'posted' WHERE id = 'je_bad'",
        [],
    );
    assert!(res.is_err());
}
