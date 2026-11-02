use accounting_kernel::{
    AccountElement, AccountRole, EntityVisibility, JournalEntryDraft, KernelError, LedgerStore,
    Money, PostingPrimitive, PostingSide, SqliteLedgerAdapter,
};

fn store() -> SqliteLedgerAdapter {
    let mut store = SqliteLedgerAdapter::in_memory().unwrap();
    store.register_entity("household", "Household", EntityVisibility::Transparent).unwrap();
    for (id, element, role) in [
        ("checking", AccountElement::Asset, AccountRole::Standard),
        ("savings", AccountElement::Asset, AccountRole::Standard),
        ("clearing", AccountElement::Asset, AccountRole::Clearing),
    ] {
        store.create_ledger_account(id, "household", id, "USD", element, role).unwrap();
    }
    store
}

fn entry(debit: &str, credit: &str, amount: i64) -> (JournalEntryDraft, String) {
    let clearing = if debit == "clearing" { debit } else { credit };
    let debit_post = PostingPrimitive::new(debit, "household", Money::new(amount, "USD").unwrap(), PostingSide::Debit, "");
    let credit_post = PostingPrimitive::new(credit, "household", Money::new(amount, "USD").unwrap(), PostingSide::Credit, "");
    let clearing_id = if clearing == debit { debit_post.id.clone() } else { credit_post.id.clone() };
    (JournalEntryDraft::new("2026-10-01", "Transfer", vec![debit_post, credit_post]), clearing_id)
}

#[test]
fn a_required_group_is_invisible_until_the_whole_transaction_commits() {
    let mut store = store();
    let (out, out_clearing) = entry("clearing", "checking", 100);
    let (incoming, in_clearing) = entry("savings", "clearing", 100);
    {
        let mut unit = store.begin_transaction(0).unwrap();
        unit.post_entry(&out).unwrap();
        assert!(unit.settle_group("transfer", "Transfer", &[&out_clearing]).is_err());
        assert!(matches!(unit.commit(), Err(KernelError::TransactionAborted)));
    }
    assert!(store.get_entry(&out.id).unwrap().is_none());
    assert_eq!(store.get_account_balance("checking").unwrap(), 0);
    assert_eq!(store.revision().unwrap(), 0);

    let mut unit = store.begin_transaction(0).unwrap();
    unit.post_entry(&out).unwrap();
    unit.post_entry(&incoming).unwrap();
    unit.settle_group("transfer", "Transfer", &[&out_clearing, &in_clearing]).unwrap();
    assert_eq!(unit.commit().unwrap(), 1);
    assert_eq!(store.get_account_balance("checking").unwrap(), -100);
    assert_eq!(store.get_account_balance("savings").unwrap(), 100);
    assert_eq!(store.get_account_balance("clearing").unwrap(), 0);
    assert!(matches!(store.begin_transaction(0), Err(KernelError::RevisionConflict { .. })));
}

#[test]
fn a_settled_entry_cannot_be_replaced_in_isolation() {
    let mut store = store();
    let (out, out_clearing) = entry("clearing", "checking", 100);
    let (incoming, in_clearing) = entry("savings", "clearing", 100);
    let mut unit = store.begin_transaction(0).unwrap();
    unit.post_entry(&out).unwrap();
    unit.post_entry(&incoming).unwrap();
    unit.settle_group("transfer", "Transfer", &[&out_clearing, &in_clearing]).unwrap();
    unit.commit().unwrap();

    let (replacement, _) = entry("clearing", "checking", 50);
    let replacement = JournalEntryDraft::with_replacement(
        replacement.posting_date,
        replacement.description,
        out.id.clone(),
        replacement.postings,
    );
    assert!(matches!(store.post_entry(&replacement), Err(KernelError::InvalidSupersedeTarget { .. })));
    assert_eq!(store.get_account_balance("clearing").unwrap(), 0);
    assert_eq!(store.revision().unwrap(), 1);
}

#[test]
fn a_saved_database_reopens_without_recreating_tables() {
    let path = std::env::temp_dir().join(format!("accounting-kernel-reopen-{}-{}.db", std::process::id(), accounting_kernel::next_deterministic_id("db")));
    let first = SqliteLedgerAdapter::open(&path).unwrap();
    assert_eq!(first.revision().unwrap(), 0);
    drop(first);
    let reopened = SqliteLedgerAdapter::open(&path).unwrap();
    assert_eq!(reopened.revision().unwrap(), 0);
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn superseding_the_only_posting_keeps_the_old_account_at_zero() {
    let mut store = store();
    let (first, _) = entry("checking", "savings", 100);
    store.post_entry(&first).unwrap();
    let replacement = JournalEntryDraft::with_replacement(
        "2026-10-01",
        "Reclassified",
        first.id,
        vec![
            PostingPrimitive::new("clearing", "household", Money::new(100, "USD").unwrap(), PostingSide::Debit, ""),
            PostingPrimitive::new("savings", "household", Money::new(100, "USD").unwrap(), PostingSide::Credit, ""),
        ],
    );
    store.post_entry(&replacement).unwrap();
    assert!(store.get_account("checking").unwrap().is_some());
    assert_eq!(store.get_account_balance("checking").unwrap(), 0);
}

#[test]
fn an_entry_that_overflows_committed_totals_is_rolled_back() {
    let mut store = store();
    let (first, _) = entry("checking", "savings", 5_000_000_000_000_000_000);
    let (second, _) = entry("checking", "savings", 5_000_000_000_000_000_000);
    store.post_entry(&first).unwrap();
    assert!(matches!(store.post_entry(&second), Err(KernelError::ArithmeticOverflow)));
    assert!(store.get_entry(&second.id).unwrap().is_none());
    assert_eq!(store.get_account_balance("checking").unwrap(), 5_000_000_000_000_000_000);
}

#[test]
fn application_persistence_failure_rolls_back_ledger_and_domain_record() {
    let mut store = store();
    store.raw_connection().execute_batch(
        "CREATE TABLE purchase_record (id TEXT PRIMARY KEY, note TEXT NOT NULL);",
    ).unwrap();
    let (purchase, _) = entry("checking", "savings", 100);
    {
        let mut unit = store.begin_transaction(0).unwrap();
        unit.post_entry(&purchase).unwrap();
        unit.with_connection(|conn| conn.execute(
            "INSERT INTO purchase_record (id, note) VALUES ('purchase-1', 'draft')", [],
        ).map(|_| ()).map_err(|e| KernelError::Storage(e.to_string()))).unwrap();
        // Simulate provenance or audit persistence failing after the ledger write.
        assert!(unit.with_connection(|conn| conn.execute(
            "INSERT INTO purchase_record (id, note) VALUES ('purchase-1', 'duplicate')", [],
        ).map(|_| ()).map_err(|e| KernelError::Storage(e.to_string()))).is_err());
        assert!(matches!(unit.commit(), Err(KernelError::TransactionAborted)));
    }
    assert!(store.get_entry(&purchase.id).unwrap().is_none());
    let count: i64 = store.raw_connection().query_row(
        "SELECT COUNT(*) FROM purchase_record", [], |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 0);
    assert_eq!(store.revision().unwrap(), 0);
}
