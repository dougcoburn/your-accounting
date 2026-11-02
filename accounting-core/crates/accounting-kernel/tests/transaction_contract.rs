use accounting_kernel::{
    AccountElement, AccountRole, EntityVisibility, EntryFilter, JournalEntryDraft, KernelError,
    LedgerStore, Money, PostingPrimitive, PostingSide, SqliteLedgerAdapter,
};

fn store() -> SqliteLedgerAdapter {
    let mut store = SqliteLedgerAdapter::in_memory().unwrap();
    store
        .register_entity("household", "Household", EntityVisibility::Transparent)
        .unwrap();
    for (id, element, role) in [
        ("checking", AccountElement::Asset, AccountRole::Standard),
        ("savings", AccountElement::Asset, AccountRole::Standard),
        ("clearing", AccountElement::Asset, AccountRole::Clearing),
    ] {
        store
            .create_ledger_account(id, "household", id, "USD", element, role)
            .unwrap();
    }
    store
}

fn entry(debit: &str, credit: &str, amount: i64) -> (JournalEntryDraft, usize) {
    let clearing_index = if debit == "clearing" { 0 } else { 1 };
    let debit_post = PostingPrimitive::new(
        debit,
        "household",
        Money::new(amount, "USD").unwrap(),
        PostingSide::Debit,
        "",
    );
    let credit_post = PostingPrimitive::new(
        credit,
        "household",
        Money::new(amount, "USD").unwrap(),
        PostingSide::Credit,
        "",
    );
    (
        JournalEntryDraft::new("2026-10-01", "Transfer", vec![debit_post, credit_post]),
        clearing_index,
    )
}

#[test]
fn a_required_group_is_invisible_until_the_whole_transaction_commits() {
    let mut store = store();
    let (out, out_index) = entry("clearing", "checking", 100);
    let (incoming, in_index) = entry("savings", "clearing", 100);
    {
        let mut unit = store.begin_transaction(0).unwrap();
        let posted_out = unit.post_entry_with_ids(&out).unwrap();
        let out_clearing = posted_out.postings[out_index].id.clone();
        assert!(unit
            .settle_group("transfer", "Transfer", &[&out_clearing])
            .is_err());
        assert!(matches!(
            unit.commit(),
            Err(KernelError::TransactionAborted)
        ));
    }
    assert!(store
        .list_active_entries_for_account("checking", &EntryFilter::new())
        .unwrap()
        .is_empty());
    assert_eq!(store.get_account_balance("checking").unwrap(), 0);
    assert_eq!(store.revision().unwrap(), 0);

    let mut unit = store.begin_transaction(0).unwrap();
    let posted_out = unit.post_entry_with_ids(&out).unwrap();
    let posted_incoming = unit.post_entry_with_ids(&incoming).unwrap();
    let out_clearing = posted_out.postings[out_index].id.clone();
    let in_clearing = posted_incoming.postings[in_index].id.clone();
    unit.settle_group("transfer", "Transfer", &[&out_clearing, &in_clearing])
        .unwrap();
    assert_eq!(unit.commit().unwrap(), 1);
    assert_eq!(store.get_account_balance("checking").unwrap(), -100);
    assert_eq!(store.get_account_balance("savings").unwrap(), 100);
    assert_eq!(store.get_account_balance("clearing").unwrap(), 0);
    assert!(matches!(
        store.begin_transaction(0),
        Err(KernelError::RevisionConflict { .. })
    ));
}

#[test]
fn a_settled_entry_cannot_be_replaced_in_isolation() {
    let mut store = store();
    let (out, out_index) = entry("clearing", "checking", 100);
    let (incoming, in_index) = entry("savings", "clearing", 100);
    let mut unit = store.begin_transaction(0).unwrap();
    let posted_out = unit.post_entry_with_ids(&out).unwrap();
    let posted_incoming = unit.post_entry_with_ids(&incoming).unwrap();
    let out_clearing = posted_out.postings[out_index].id.clone();
    let in_clearing = posted_incoming.postings[in_index].id.clone();
    unit.settle_group("transfer", "Transfer", &[&out_clearing, &in_clearing])
        .unwrap();
    unit.commit().unwrap();

    let (replacement, _) = entry("clearing", "checking", 50);
    let replacement = JournalEntryDraft::with_replacement(
        replacement.posting_date,
        replacement.description,
        posted_out.id,
        replacement.postings,
    );
    assert!(matches!(
        store.post_entry(&replacement),
        Err(KernelError::InvalidSupersedeTarget { .. })
    ));
    assert_eq!(store.get_account_balance("clearing").unwrap(), 0);
    assert_eq!(store.revision().unwrap(), 1);
}

#[test]
fn a_saved_database_reopens_without_recreating_tables() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "accounting-kernel-reopen-{}-{nonce}.db",
        std::process::id()
    ));
    let mut first = SqliteLedgerAdapter::open(&path).unwrap();
    assert_eq!(first.next_deterministic_id("je").unwrap(), "je_1");
    assert_eq!(first.revision().unwrap(), 0);
    drop(first);
    let mut reopened = SqliteLedgerAdapter::open(&path).unwrap();
    assert_eq!(reopened.next_deterministic_id("je").unwrap(), "je_2");
    assert_eq!(reopened.revision().unwrap(), 0);
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn short_ids_are_per_prefix_and_roll_back_with_the_unit_of_work() {
    let mut store = store();
    assert_eq!(store.next_deterministic_id("je").unwrap(), "je_1");
    assert_eq!(store.next_deterministic_id("pst").unwrap(), "pst_1");
    assert_eq!(store.next_deterministic_id("je").unwrap(), "je_2");
    let revision = store.revision().unwrap();
    {
        let mut unit = store.begin_transaction(revision).unwrap();
        assert_eq!(unit.next_deterministic_id("je").unwrap(), "je_3");
    }
    assert_eq!(store.next_deterministic_id("je").unwrap(), "je_3");
    assert!(matches!(
        store.next_deterministic_id("bad-prefix!"),
        Err(KernelError::InvalidIdPrefix(_))
    ));
}

#[test]
fn migration_continues_after_existing_numeric_ids() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("accounting-id-migration-{nonce}.sqlite"));
    {
        let mut store = SqliteLedgerAdapter::open(&path).unwrap();
        store
            .register_entity("household", "Household", EntityVisibility::Transparent)
            .unwrap();
        for id in ["checking", "savings"] {
            store
                .create_ledger_account(
                    id,
                    "household",
                    id,
                    "USD",
                    AccountElement::Asset,
                    AccountRole::Standard,
                )
                .unwrap();
        }
        let (draft, _) = entry("checking", "savings", 100);
        assert_eq!(store.post_entry_with_ids(&draft).unwrap().id, "je_1");
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE id_sequences; PRAGMA user_version = 3;")
            .unwrap();
    }
    let mut reopened = SqliteLedgerAdapter::open(&path).unwrap();
    let (draft, _) = entry("checking", "savings", 200);
    let posted = reopened.post_entry_with_ids(&draft).unwrap();
    assert_eq!(posted.id, "je_2");
    assert_eq!(posted.postings[0].id, "pst_3");
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn posting_replaces_draft_references_with_storage_ids() {
    let mut store = store();
    let (draft, _) = entry("checking", "savings", 100);
    assert!(draft.id.starts_with("~draft_je_"));
    assert!(draft.postings[0].id.starts_with("~draft_pst_"));
    let posted = store.post_entry_with_ids(&draft).unwrap();
    assert_eq!(posted.id, "je_1");
    assert_eq!(posted.postings[0].id, "pst_1");
    assert_eq!(posted.postings[1].id, "pst_2");
}

#[test]
fn application_reserves_ids_before_posting_and_keeps_them() {
    let mut store = store();
    let mut unit = store.begin_transaction(0).unwrap();
    let entry_id = unit.next_deterministic_id("je").unwrap();
    let debit_id = unit.next_deterministic_id("pst").unwrap();
    let credit_id = unit.next_deterministic_id("pst").unwrap();
    let application_id = unit.next_deterministic_id("purchase").unwrap();
    let draft = JournalEntryDraft::new(
        "2026-10-01",
        "Purchase",
        vec![
            PostingPrimitive::new(
                "checking",
                "household",
                Money::new(100, "USD").unwrap(),
                PostingSide::Debit,
                "",
            )
            .with_id(&debit_id),
            PostingPrimitive::new(
                "savings",
                "household",
                Money::new(100, "USD").unwrap(),
                PostingSide::Credit,
                "",
            )
            .with_id(&credit_id),
        ],
    )
    .with_id(&entry_id);
    unit.with_connection(|conn| {
        conn.execute_batch("CREATE TABLE purchases (id TEXT PRIMARY KEY, entry_id TEXT NOT NULL REFERENCES journal_entries(id))")
            .map_err(|e| KernelError::Storage(e.to_string()))
    }).unwrap();
    assert_eq!(unit.post_entry(&draft).unwrap(), entry_id);
    unit.with_connection(|conn| {
        conn.execute(
            "INSERT INTO purchases (id, entry_id) VALUES (?1, ?2)",
            [&application_id, &entry_id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(unit.commit().unwrap(), 1);
    let posted = store.get_entry(&entry_id).unwrap().unwrap();
    assert_eq!(posted.postings[0].id, debit_id);
    assert_eq!(posted.postings[1].id, credit_id);
    assert_eq!(application_id, "purchase_1");
    assert!(matches!(
        store.post_entry(&draft),
        Err(KernelError::DuplicateId {
            record_kind: "journal_entry",
            ..
        })
    ));
}

#[test]
fn superseding_the_only_posting_keeps_the_old_account_at_zero() {
    let mut store = store();
    let (first, _) = entry("checking", "savings", 100);
    let first_id = store.post_entry(&first).unwrap();
    let replacement = JournalEntryDraft::with_replacement(
        "2026-10-01",
        "Reclassified",
        first_id,
        vec![
            PostingPrimitive::new(
                "clearing",
                "household",
                Money::new(100, "USD").unwrap(),
                PostingSide::Debit,
                "",
            ),
            PostingPrimitive::new(
                "savings",
                "household",
                Money::new(100, "USD").unwrap(),
                PostingSide::Credit,
                "",
            ),
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
    assert!(matches!(
        store.post_entry(&second),
        Err(KernelError::ArithmeticOverflow)
    ));
    assert_eq!(store.revision().unwrap(), 1);
    assert_eq!(
        store.get_account_balance("checking").unwrap(),
        5_000_000_000_000_000_000
    );
}

#[test]
fn application_persistence_failure_rolls_back_ledger_and_domain_record() {
    let mut store = store();
    store
        .raw_connection()
        .execute_batch("CREATE TABLE purchase_record (id TEXT PRIMARY KEY, note TEXT NOT NULL);")
        .unwrap();
    let (purchase, _) = entry("checking", "savings", 100);
    {
        let mut unit = store.begin_transaction(0).unwrap();
        unit.post_entry(&purchase).unwrap();
        unit.with_connection(|conn| {
            conn.execute(
                "INSERT INTO purchase_record (id, note) VALUES ('purchase-1', 'draft')",
                [],
            )
            .map(|_| ())
            .map_err(|e| KernelError::Storage(e.to_string()))
        })
        .unwrap();
        // Simulate provenance or audit persistence failing after the ledger write.
        assert!(unit
            .with_connection(|conn| conn
                .execute(
                    "INSERT INTO purchase_record (id, note) VALUES ('purchase-1', 'duplicate')",
                    [],
                )
                .map(|_| ())
                .map_err(|e| KernelError::Storage(e.to_string())))
            .is_err());
        assert!(matches!(
            unit.commit(),
            Err(KernelError::TransactionAborted)
        ));
    }
    assert!(store
        .list_active_entries_for_account("checking", &EntryFilter::new())
        .unwrap()
        .is_empty());
    let count: i64 = store
        .raw_connection()
        .query_row("SELECT COUNT(*) FROM purchase_record", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(store.revision().unwrap(), 0);
}
