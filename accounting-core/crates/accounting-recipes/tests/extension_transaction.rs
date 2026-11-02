use accounting_kernel::{
    AccountElement, AccountRole, EntityVisibility, JournalEntryDraft, KernelError, LedgerStore,
    Money, PostingPrimitive, PostingSide, SqliteLedgerAdapter, SqliteLedgerTransaction,
};
use accounting_recipes::{RecipeCompiler, RecipeExecution, RECIPES_SCHEMA_SQL};
use rusqlite::{params, Connection};

const APPLICATION_SCHEMA: &str = "
    CREATE TABLE categories (id TEXT PRIMARY KEY, name TEXT NOT NULL);
    CREATE TABLE tags (id TEXT PRIMARY KEY, name TEXT NOT NULL);
    CREATE TABLE entry_categories (
        entry_id TEXT NOT NULL REFERENCES journal_entries(id),
        category_id TEXT NOT NULL REFERENCES categories(id),
        PRIMARY KEY (entry_id, category_id)
    );
    CREATE TABLE entry_tags (
        entry_id TEXT NOT NULL REFERENCES journal_entries(id),
        tag_id TEXT NOT NULL REFERENCES tags(id),
        PRIMARY KEY (entry_id, tag_id)
    );";

fn stage_application_purchase(
    unit: &mut SqliteLedgerTransaction<'_>,
    inject_failure: bool,
) -> Result<(String, String, String, String), KernelError> {
    unit.with_connection(|conn| {
        conn.execute_batch(RECIPES_SCHEMA_SQL)
            .and_then(|_| conn.execute_batch(APPLICATION_SCHEMA))
            .map_err(|e| KernelError::Storage(e.to_string()))
    })?;

    let entry_id = unit.next_deterministic_id("je")?;
    let debit_id = unit.next_deterministic_id("pst")?;
    let credit_id = unit.next_deterministic_id("pst")?;
    let recipe_id = unit.next_deterministic_id("rx")?;
    let category_id = unit.next_deterministic_id("cat")?;
    let tag_id = unit.next_deterministic_id("tag")?;
    let draft = JournalEntryDraft::new(
        "2026-11-01",
        "Groceries",
        vec![
            PostingPrimitive::new(
                "groceries",
                "home",
                Money::new(1250, "USD")?,
                PostingSide::Debit,
                "",
            )
            .with_id(debit_id),
            PostingPrimitive::new(
                "checking",
                "home",
                Money::new(1250, "USD")?,
                PostingSide::Credit,
                "",
            )
            .with_id(credit_id),
        ],
    )
    .with_id(&entry_id);
    assert_eq!(unit.post_entry(&draft)?, entry_id);

    let recipe = RecipeExecution {
        recipe_name: "categorized_purchase".into(),
        recipe_version: "1".into(),
        journal_entry_id: Some(entry_id.clone()),
        parameters_json: "{\"amount_minor\":1250}".into(),
        executed_at: None,
    };
    unit.with_connection(|conn| {
        RecipeCompiler::record_execution(conn, &recipe_id, &recipe, &entry_id)?;
        conn.execute(
            "INSERT INTO categories (id, name) VALUES (?1, 'Food')",
            [&category_id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;
        conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1, 'Weekly')",
            [&tag_id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;
        conn.execute(
            "INSERT INTO entry_categories (entry_id, category_id) VALUES (?1, ?2)",
            params![entry_id, category_id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag_id) VALUES (?1, ?2)",
            params![entry_id, tag_id],
        )
        .map_err(|e| KernelError::Storage(e.to_string()))?;
        if inject_failure {
            conn.execute(
                "INSERT INTO tags (id, name) VALUES (?1, 'Duplicate')",
                [&tag_id],
            )
            .map_err(|e| KernelError::Storage(e.to_string()))?;
        }
        Ok(())
    })?;
    Ok((entry_id, recipe_id, category_id, tag_id))
}

#[test]
fn extension_schema_and_records_share_the_kernel_commit_boundary() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("accounting-extension-{nonce}.sqlite"));
    let mut store = SqliteLedgerAdapter::open(&path).unwrap();
    store
        .register_entity("home", "Home", EntityVisibility::Transparent)
        .unwrap();
    store
        .create_ledger_account(
            "checking",
            "home",
            "Checking",
            "USD",
            AccountElement::Asset,
            AccountRole::Standard,
        )
        .unwrap();
    store
        .create_ledger_account(
            "groceries",
            "home",
            "Groceries",
            "USD",
            AccountElement::Expense,
            AccountRole::Standard,
        )
        .unwrap();

    let mut failed_unit = store.begin_transaction(0).unwrap();
    assert!(stage_application_purchase(&mut failed_unit, true).is_err());
    assert!(matches!(
        failed_unit.commit(),
        Err(KernelError::TransactionAborted)
    ));
    drop(store);

    let mut reopened = SqliteLedgerAdapter::open(&path).unwrap();
    assert_eq!(reopened.revision().unwrap(), 0);
    assert_eq!(reopened.get_account_balance("checking").unwrap(), 0);
    assert!(reopened.get_entry("je_1").unwrap().is_none());
    let other = Connection::open(&path).unwrap();
    let tables: i64 = other.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('recipe_executions', 'categories', 'tags', 'entry_categories', 'entry_tags')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(tables, 0);
    drop(other);

    let mut successful_unit = reopened.begin_transaction(0).unwrap();
    let (entry_id, recipe_id, category_id, tag_id) =
        stage_application_purchase(&mut successful_unit, false).unwrap();
    assert_eq!(
        (&*entry_id, &*recipe_id, &*category_id, &*tag_id),
        ("je_1", "rx_1", "cat_1", "tag_1")
    );

    let other = Connection::open(&path).unwrap();
    assert!(other
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE name = 'recipe_executions'",
            [],
            |_| Ok(())
        )
        .is_err());
    assert_eq!(
        other
            .query_row("SELECT COUNT(*) FROM journal_entries", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(other);

    assert_eq!(successful_unit.commit().unwrap(), 1);
    drop(reopened);
    let reopened = SqliteLedgerAdapter::open(&path).unwrap();
    assert_eq!(reopened.revision().unwrap(), 1);
    assert_eq!(reopened.get_account_balance("checking").unwrap(), -1250);
    assert_eq!(
        reopened
            .get_entry(&entry_id)
            .unwrap()
            .unwrap()
            .postings
            .len(),
        2
    );
    let other = Connection::open(&path).unwrap();
    let linked: (String, String, String, String) = other
        .query_row(
            "SELECT rx.id, c.id, t.id, rx.journal_entry_id
         FROM recipe_executions rx
         JOIN entry_categories ec ON ec.entry_id = rx.journal_entry_id
         JOIN categories c ON c.id = ec.category_id
         JOIN entry_tags et ON et.entry_id = rx.journal_entry_id
         JOIN tags t ON t.id = et.tag_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(linked, (recipe_id, category_id, tag_id, entry_id));
    drop(other);
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}
