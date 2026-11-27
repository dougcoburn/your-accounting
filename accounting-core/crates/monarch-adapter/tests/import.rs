//! U23: the synthetic October fixture imports 1:1, and a second import adds nothing.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use accounting_kernel::v2::{
    observation_links, AccountType, FinancialKind, FixedClock, Kernel, LedgerKind, NewEntity,
    NewEvent, NewFinancialAccount, NewLedgerAccount, NewPosting, ObservationImport,
    ObservationLink, SequenceIds, Side, TrackingMode,
};
use monarch_adapter::{
    import_monarch_csv, month1_category_classes, ImportRequest, EXPENSE_PARENT, INCOME_PARENT,
};

const WHEN: &str = "2026-01-02T03:04:05Z";
const FEED: &str = "01ARZ3NDEKTSV4RRFFQ69GFEED";
const HEADER: &str = "Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id";

struct Tmp {
    dir: PathBuf,
}

impl Tmp {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("ya-u23-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn open(dir: &Path) -> Kernel {
    let evidence = dir.join("evidence");
    std::fs::create_dir_all(&evidence).expect("evidence");
    let import = ObservationImport::new(&evidence);
    let mut kernel = Kernel::open_with(
        dir.join("entity.sqlite"),
        Box::new(FixedClock {
            timestamp: WHEN.to_string(),
        }),
        Box::new(SequenceIds::new()),
        "u23",
        vec![import.participant()],
    )
    .expect("open");
    import.register_doctor_checks(&mut kernel);
    kernel
}

fn request<'a>(
    csv: &'a str,
    revision: i64,
    classes: &'a [monarch_adapter::CategoryClass],
) -> ImportRequest<'a> {
    ImportRequest {
        csv,
        feed_id: FEED,
        feed_name: "Synthetic October",
        expected_revision: revision,
        categories: classes,
    }
}

struct Book {
    name: &'static str,
    last4: &'static str,
    kind: FinancialKind,
}

fn october_books() -> [Book; 12] {
    [
        Book {
            name: "Bramble Brokerage ...3160",
            last4: "3160",
            kind: FinancialKind::Brokerage,
        },
        Book {
            name: "Cinder Mastercard ...5590",
            last4: "5590",
            kind: FinancialKind::CreditCard,
        },
        Book {
            name: "Cinder Visa ...5582",
            last4: "5582",
            kind: FinancialKind::CreditCard,
        },
        Book {
            name: "Dragonfly Operating ...9936",
            last4: "9936",
            kind: FinancialKind::Checking,
        },
        Book {
            name: "Fable Rewards ...6603",
            last4: "6603",
            kind: FinancialKind::CreditCard,
        },
        Book {
            name: "Harbor Joint Checking ...3340",
            last4: "3340",
            kind: FinancialKind::Checking,
        },
        Book {
            name: "Lumen Cash ...4471",
            last4: "4471",
            kind: FinancialKind::Cash,
        },
        Book {
            name: "Nimbus Store Card ...7714",
            last4: "7714",
            kind: FinancialKind::CreditCard,
        },
        Book {
            name: "Oak Mortgage ...1048",
            last4: "1048",
            kind: FinancialKind::Loan,
        },
        Book {
            name: "Pebble Checking ...2210",
            last4: "2210",
            kind: FinancialKind::Checking,
        },
        Book {
            name: "Pebble Savings ...2218",
            last4: "2218",
            kind: FinancialKind::Savings,
        },
        Book {
            name: "Sable Auto Loan ...2059",
            last4: "2059",
            kind: FinancialKind::Loan,
        },
    ]
}

fn setup_october(kernel: &mut Kernel) -> i64 {
    let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
    tx.create_entity(NewEntity {
        id: None,
        party_id: None,
        name: "Household".to_string(),
    })
    .expect("entity");
    for book in october_books() {
        tx.create_financial_account(NewFinancialAccount {
            id: None,
            name: book.name.to_string(),
            kind: book.kind,
            currency: "USD".to_string(),
            tracking_mode: TrackingMode::Transparent,
            institution: None,
            last4: Some(book.last4.to_string()),
        })
        .expect("account");
    }
    tx.commit().expect("commit").seq
}

fn setup_checking(kernel: &mut Kernel) -> (i64, String) {
    let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
    tx.create_entity(NewEntity {
        id: None,
        party_id: None,
        name: "Household".to_string(),
    })
    .expect("entity");
    let financial = tx
        .create_financial_account(NewFinancialAccount {
            id: None,
            name: "Checking ...1234".to_string(),
            kind: FinancialKind::Checking,
            currency: "USD".to_string(),
            tracking_mode: TrackingMode::Transparent,
            institution: None,
            last4: Some("1234".to_string()),
        })
        .expect("financial");
    let cash = tx
        .create_ledger_account(NewLedgerAccount {
            id: None,
            financial_account_id: Some(financial),
            account_type: AccountType::Asset,
            kind: LedgerKind::Cash,
            parent_id: None,
            name: "Checking ...1234".to_string(),
        })
        .expect("cash");
    let seq = tx.commit().expect("commit").seq;
    (seq, cash)
}

fn tiny(amount: &str, pending: &str, reviewed: &str, id: &str) -> String {
    format!(
        "# synthetic\n{HEADER}\n2025-10-02,Northwind Market,Groceries,Checking ...1234,POS NORTHWIND,,{amount},e-household,Casey Sample,{reviewed},2025-10-02 10:00:00,Rowan Checker,{pending},{id}\n"
    )
}

fn hint(payload: &str, needle: &str) {
    assert!(payload.contains(needle), "missing hint");
}

#[test]
fn fixture_imports_one_to_one_and_reimport_is_a_noop() {
    let tmp = Tmp::new("fixture");
    let mut kernel = open(tmp.path());
    let revision = setup_october(&mut kernel);
    let csv = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/monarch/october-2025-synthetic.csv"),
    )
    .expect("fixture");
    let classes = month1_category_classes();
    let first =
        import_monarch_csv(&mut kernel, &request(&csv, revision, &classes)).expect("import");
    assert_eq!(first.data_rows, 200);
    assert_eq!(first.inserted, 200);
    assert_eq!(first.observation_ids.len(), 200);
    assert_eq!(first.unchanged, 0);
    assert!(first.withheld_pending.is_empty());
    assert!(first.proposals.is_empty());
    assert!(first.leaves_created > 0);
    assert!(!first.noop);
    assert_eq!(kernel.observation_count().expect("count"), 200);
    assert_eq!(kernel.tag_count().expect("tags"), 0);
    assert_eq!(kernel.posting_tag_count().expect("posting tags"), 0);
    let commit = kernel
        .commit_at_seq(first.seq)
        .expect("commit read")
        .expect("commit");
    assert_eq!(commit.kind, "import");

    let card = kernel
        .observation_by_external_id(FEED, "s-card-01")
        .expect("read")
        .expect("card");
    assert_eq!(card.direction, "outflow");
    assert_eq!(card.amount_minor, 6418);
    assert_eq!(card.occurred_date, "2025-10-02");
    assert_eq!(card.posted_date.as_deref(), Some("2025-10-02"));
    assert_eq!(card.currency, "USD");
    hint(&card.payload_json, "\"reviewed_authoritative\":false");
    hint(&card.payload_json, "\"e_tag_hints\":[\"e-household\"]");
    assert_eq!(
        kernel
            .observation_review_state(&card.id)
            .expect("review")
            .as_deref(),
        Some("unreviewed")
    );

    let pay = kernel
        .observation_by_external_id(FEED, "s-pay-01")
        .expect("read")
        .expect("pay");
    assert_eq!(pay.direction, "inflow");
    assert_eq!(pay.amount_minor, 218_042);
    assert_eq!(pay.occurred_date, "2025-10-03");
    assert_eq!(pay.posted_date.as_deref(), Some("2025-10-03"));
    hint(&pay.payload_json, "\"e_tag_hints\":[\"e-acme\"]");
    hint(&pay.payload_json, "\"untagged_rule\":null");

    let open_row = kernel
        .observation_by_external_id(FEED, "sf078")
        .expect("read")
        .expect("untagged");
    hint(
        &open_row.payload_json,
        "\"untagged_rule\":\"account_owner_or_same_owner_transfer\"",
    );
    hint(&open_row.payload_json, "\"e_tag_hints\":[]");

    for id in ["s-hsa-01", "s-perk-01", "s-card-03", "s-dup-a", "s-dup-b"] {
        assert!(
            kernel
                .observation_by_external_id(FEED, id)
                .expect("read")
                .is_some(),
            "missing scenario row"
        );
    }

    let groceries = kernel
        .ledger_categories_named("Groceries")
        .expect("groceries");
    assert_eq!(groceries.len(), 1);
    assert_eq!(groceries[0].account_type, "expense");
    let expense = kernel
        .ledger_categories_named(EXPENSE_PARENT)
        .expect("expense parent");
    let parent_id = expense
        .iter()
        .find(|row| row.parent_id.is_none())
        .expect("parent")
        .id
        .clone();
    assert_eq!(groceries[0].parent_id.as_deref(), Some(parent_id.as_str()));

    let paychecks = kernel.ledger_categories_named("Paychecks").expect("pay");
    assert_eq!(paychecks.len(), 1);
    assert_eq!(paychecks[0].account_type, "income");
    let income = kernel
        .ledger_categories_named(INCOME_PARENT)
        .expect("income");
    let income_id = income
        .iter()
        .find(|row| row.parent_id.is_none())
        .expect("income parent")
        .id
        .clone();
    assert_eq!(paychecks[0].parent_id.as_deref(), Some(income_id.as_str()));

    for name in [
        "Credit Card Payment",
        "Transfer",
        "HSA Contribution",
        "Reimbursements",
    ] {
        assert!(
            kernel
                .ledger_categories_named(name)
                .expect("lookup")
                .is_empty(),
            "category was seeded"
        );
    }

    let second =
        import_monarch_csv(&mut kernel, &request(&csv, first.seq, &classes)).expect("reimport");
    assert_eq!(second.inserted, 0);
    assert_eq!(second.unchanged, 200);
    assert_eq!(second.leaves_created, 0);
    assert!(second.proposals.is_empty());
    assert!(second.noop);
    assert_eq!(second.seq, first.seq);
    assert_eq!(kernel.observation_count().expect("count"), 200);
    let doctor = kernel.doctor(Some(second.seq)).expect("doctor");
    assert_eq!(doctor.exit_status, 0, "{:?}", doctor.findings);
}

#[test]
fn pending_rows_are_withheld_until_a_later_export_clears_the_flag() {
    let tmp = Tmp::new("pending");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let classes = month1_category_classes();
    let held = tiny("-12.50", "true", "true", "nw-01");
    let first = import_monarch_csv(&mut kernel, &request(&held, revision, &classes)).expect("hold");
    assert_eq!(first.withheld_pending, vec![3]);
    assert_eq!(first.inserted, 0);
    assert!(first.noop);
    assert_eq!(kernel.observation_count().expect("count"), 0);
    assert_eq!(kernel.revision().expect("rev"), revision);

    let posted = tiny("-12.50", "false", "true", "nw-01");
    let second =
        import_monarch_csv(&mut kernel, &request(&posted, revision, &classes)).expect("post");
    assert_eq!(second.inserted, 1);
    assert!(second.withheld_pending.is_empty());
    let fact = kernel
        .observation_by_external_id(FEED, "nw-01")
        .expect("read")
        .expect("row");
    assert_eq!(fact.amount_minor, 1250);
    assert_eq!(fact.direction, "outflow");
    hint(&fact.payload_json, "\"pending_authoritative\":false");
    hint(&fact.payload_json, "\"reviewed_authoritative\":false");
    assert_eq!(
        kernel
            .observation_review_state(&fact.id)
            .expect("review")
            .as_deref(),
        Some("unreviewed")
    );

    let again =
        import_monarch_csv(&mut kernel, &request(&posted, second.seq, &classes)).expect("again");
    assert!(again.noop);
    assert_eq!(again.inserted, 0);
    assert_eq!(again.unchanged, 1);
    assert_eq!(kernel.observation_count().expect("count"), 1);
}

#[test]
fn a_changed_export_keeps_the_stored_row_and_names_the_column() {
    let tmp = Tmp::new("change");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let classes = month1_category_classes();
    let original = format!(
        "# synthetic\n{HEADER}\n2025-10-02,Northwind Market,Groceries,Checking ...1234,POS NORTHWIND,,-12.50,e-household,Casey Sample,true,2025-10-02 10:00:00,Rowan Checker,false,nw-01\n2025-10-03,Harbor Books,Shopping,Checking ...1234,POS HARBOR,,-3.00,,Casey Sample,false,2025-10-03 11:00:00,Rowan Checker,false,nw-02\n"
    );
    let first =
        import_monarch_csv(&mut kernel, &request(&original, revision, &classes)).expect("first");
    assert_eq!(first.inserted, 2);
    let changed = original.replace("-12.50", "-13.50");
    let second =
        import_monarch_csv(&mut kernel, &request(&changed, first.seq, &classes)).expect("second");
    assert_eq!(second.inserted, 0);
    assert_eq!(second.proposals.len(), 1);
    assert_eq!(second.proposals[0].row, 3);
    assert_eq!(second.proposals[0].columns, vec!["Amount"]);
    let shown = format!(
        "row={} column={}",
        second.proposals[0].row, second.proposals[0].columns[0]
    );
    assert!(!shown.contains("13.50"));
    assert!(!shown.contains("Northwind"));
    let kept = kernel
        .observation_by_external_id(FEED, "nw-01")
        .expect("read")
        .expect("kept");
    assert_eq!(kept.amount_minor, 1250);
    let sibling = kernel
        .observation_by_external_id(FEED, "nw-02")
        .expect("read")
        .expect("sibling");
    hint(&sibling.payload_json, "\"reviewed_hint\":false");
    hint(&sibling.payload_json, "\"reviewed_authoritative\":false");
    assert_eq!(
        kernel
            .observation_review_state(&sibling.id)
            .expect("review")
            .as_deref(),
        Some("unreviewed")
    );
}

#[test]
fn a_new_row_still_lands_when_another_row_conflicts() {
    let tmp = Tmp::new("partial");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let classes = month1_category_classes();
    let original = tiny("-12.50", "false", "true", "nw-01");
    let first =
        import_monarch_csv(&mut kernel, &request(&original, revision, &classes)).expect("first");
    let both = format!(
        "{original}2025-10-04,Harbor Books,Shopping,Checking ...1234,POS HARBOR,,-3.00,e-household,Casey Sample,true,2025-10-04 11:00:00,Rowan Checker,false,nw-02\n"
    )
    .replace("-12.50", "-9.00");
    let second =
        import_monarch_csv(&mut kernel, &request(&both, first.seq, &classes)).expect("second");
    assert_eq!(second.inserted, 1);
    assert_eq!(second.proposals.len(), 1);
    assert_eq!(second.proposals[0].columns, vec!["Amount"]);
    assert_eq!(
        kernel
            .observation_by_external_id(FEED, "nw-01")
            .expect("read")
            .expect("old")
            .amount_minor,
        1250
    );
    assert_eq!(
        kernel
            .observation_by_external_id(FEED, "nw-02")
            .expect("read")
            .expect("new")
            .amount_minor,
        300
    );
}

#[test]
fn malformed_cells_name_the_row_and_column_and_write_nothing() {
    let tmp = Tmp::new("bad");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let classes = month1_category_classes();
    let bad_amount = tiny("nope", "false", "true", "nw-01");
    let err = import_monarch_csv(&mut kernel, &request(&bad_amount, revision, &classes))
        .expect_err("amount");
    let shown = err.to_string();
    assert_eq!(shown, "row=3 column=Amount");
    assert!(!shown.contains("nope"));
    assert!(!shown.contains("Northwind"));
    assert_eq!(kernel.observation_count().expect("count"), 0);
    assert_eq!(kernel.revision().expect("rev"), revision);

    let bad_account = "# synthetic\n".to_string()
        + HEADER
        + "\n2025-10-02,Northwind Market,Groceries,Mystery Checking ...9999,POS,,-1.00,e-household,Casey Sample,true,2025-10-02 10:00:00,Rowan Checker,false,nw-09\n";
    let err = import_monarch_csv(&mut kernel, &request(&bad_account, revision, &classes))
        .expect_err("account");
    let shown = err.to_string();
    assert_eq!(shown, "row=3 column=Account");
    assert!(!shown.contains("Mystery"));
    assert!(!shown.contains("9999"));
    assert!(!shown.contains("Northwind"));

    let bad_header = "# synthetic\nDate,Merchant,Nope,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id\n";
    let err = import_monarch_csv(&mut kernel, &request(bad_header, revision, &classes))
        .expect_err("header");
    let shown = err.to_string();
    assert_eq!(shown, "row=2 column=Category");
    assert!(!shown.contains("Nope"));
}

#[test]
fn duplicate_ids_in_one_file_are_rejected() {
    let tmp = Tmp::new("dup");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let classes = month1_category_classes();
    let csv = format!("{}{}", tiny("-1.00", "false", "true", "nw-01"), "2025-10-03,Harbor Books,Shopping,Checking ...1234,POS,,-2.00,e-household,Casey Sample,true,2025-10-03 10:00:00,Rowan Checker,false,nw-01\n");
    let err = import_monarch_csv(&mut kernel, &request(&csv, revision, &classes)).expect_err("dup");
    assert_eq!(err.to_string(), "row=4 column=Id");
    assert_eq!(kernel.observation_count().expect("count"), 0);
}

#[test]
fn link_and_void_use_the_imported_observation() {
    let tmp = Tmp::new("void");
    let mut kernel = open(tmp.path());
    let (revision, cash) = setup_checking(&mut kernel);
    let classes = month1_category_classes();
    let csv = tiny("-12.50", "false", "true", "nw-01");
    let imported =
        import_monarch_csv(&mut kernel, &request(&csv, revision, &classes)).expect("import");
    let observation = imported.observation_ids[0].clone();
    let groceries = kernel.ledger_categories_named("Groceries").expect("cat");
    assert_eq!(groceries.len(), 1);
    let debit = "01ARZ3NDEKTSV4RRFFQ69GDBT1";
    let credit = "01ARZ3NDEKTSV4RRFFQ69GCDT1";
    let mut tx = kernel.begin_transaction(Some(imported.seq)).expect("begin");
    tx.register_recipe("u23_probe", "1").expect("recipe");
    tx.append_event_with(
        NewEvent {
            id: None,
            recipe_id: "u23_probe".to_string(),
            recipe_version: "1".to_string(),
            effective_date: "2025-10-02".to_string(),
            description: String::new(),
            postings: vec![
                NewPosting {
                    id: Some(debit.to_string()),
                    ledger_account_id: groceries[0].id.clone(),
                    currency: "USD".to_string(),
                    side: Side::Debit,
                    amount_minor: 1250,
                    party_id: None,
                },
                NewPosting {
                    id: Some(credit.to_string()),
                    ledger_account_id: cash,
                    currency: "USD".to_string(),
                    side: Side::Credit,
                    amount_minor: 1250,
                    party_id: None,
                },
            ],
            recipe_instance_id: None,
            counterpart_entity_id: None,
            expectations: Vec::new(),
        },
        observation_links(vec![ObservationLink {
            line_index: 1,
            observation_id: observation.clone(),
        }]),
    )
    .expect("link");
    let linked = tx.commit().expect("commit");
    assert_eq!(
        kernel.posting_origin(credit).expect("origin").as_deref(),
        Some("observed")
    );
    assert_eq!(kernel.posting_count().expect("postings"), 2);

    let mut tx = kernel.begin_transaction(Some(linked.seq)).expect("begin");
    tx.void_observation(&observation, "duplicate")
        .expect("void");
    tx.commit().expect("commit");
    assert_eq!(
        kernel.posting_origin(credit).expect("origin").as_deref(),
        Some("expected")
    );
    assert_eq!(kernel.posting_count().expect("postings"), 2);
    assert_eq!(
        kernel
            .observation_review_state(&observation)
            .expect("review"),
        None
    );
    assert!(kernel
        .observation_by_external_id(FEED, "nw-01")
        .expect("read")
        .is_some());
}
