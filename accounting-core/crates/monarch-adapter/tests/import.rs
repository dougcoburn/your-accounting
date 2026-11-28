//! U23: the synthetic October fixture imports 1:1, and a second import adds nothing.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use accounting_kernel::v2::{
    observation_links, AccountType, FinancialKind, FixedClock, Kernel, LedgerKind, NewEntity,
    NewEvent, NewFinancialAccount, NewLedgerAccount, NewPosting, ObservationImport,
    ObservationLink, SequenceIds, Side, TrackingMode,
};
use monarch_adapter::{import_monarch_csv, ImportRequest};

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

fn request(csv: &str, revision: i64) -> ImportRequest<'_> {
    ImportRequest {
        csv,
        feed_id: FEED,
        feed_name: "Synthetic October",
        expected_revision: revision,
        payroll_cross_check_name: Some("Payroll Tracking"),
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
    let first = import_monarch_csv(&mut kernel, &request(&csv, revision)).expect("import");
    assert_eq!(first.data_rows, 200);
    assert_eq!(first.inserted, 200);
    assert_eq!(first.observation_ids.len(), 200);
    assert_eq!(first.unchanged, 0);
    assert_eq!(first.revised, 0);
    assert_eq!(first.voided, 0);
    assert!(!first.noop);
    assert_eq!(kernel.observation_count().expect("count"), 200);
    assert_eq!(kernel.tag_count().expect("tags"), 0);
    assert_eq!(kernel.posting_tag_count().expect("posting tags"), 0);
    let commit = kernel
        .commit_at_seq(first.seq)
        .expect("commit read")
        .expect("commit");
    assert_eq!(commit.kind, "import");

    let payroll = kernel
        .observation_by_external_id(FEED, "s-pr-pos")
        .expect("read")
        .expect("payroll");
    assert!(payroll.financial_account_id.is_none());
    assert!(payroll.cross_check_account_id.is_some());
    hint(&payroll.payload_json, "\"cross_check_role\":\"payroll\"");
    assert_eq!(kernel.posting_count().expect("postings"), 0);

    let card = kernel
        .observation_by_external_id(FEED, "s-card-01")
        .expect("read")
        .expect("card");
    assert_eq!(card.direction, "outflow");
    assert_eq!(card.amount_minor, 6418);
    assert_eq!(card.occurred_date, "2025-10-02");
    assert_eq!(card.posted_date.as_deref(), Some("2025-10-02"));
    assert_eq!(card.currency, "USD");
    hint(&card.payload_json, "\"reviewed_hint\":true");
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
    let open_review = kernel
        .observation_by_external_id(FEED, "s-fly-01")
        .expect("read")
        .expect("fly");
    hint(&open_review.payload_json, "\"reviewed_hint\":false");

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

    assert!(kernel
        .ledger_categories_named("Groceries")
        .expect("groceries")
        .is_empty());
    assert!(kernel
        .ledger_categories_named("Paychecks")
        .expect("paychecks")
        .is_empty());
    assert_eq!(kernel.posting_count().expect("postings"), 0);

    let second = import_monarch_csv(&mut kernel, &request(&csv, first.seq)).expect("reimport");
    assert_eq!(second.inserted, 0);
    assert_eq!(second.revised, 0);
    assert_eq!(second.voided, 0);
    assert_eq!(second.unchanged, 200);
    assert!(second.noop);
    assert_eq!(second.seq, first.seq);
    assert_eq!(kernel.observation_count().expect("count"), 200);
    let doctor = kernel.doctor(Some(second.seq)).expect("doctor");
    assert_eq!(doctor.exit_status, 0, "{:?}", doctor.findings);
}

#[test]
fn pending_rows_are_stored_and_a_later_pull_updates_the_flag() {
    let tmp = Tmp::new("pending");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let held = tiny("-12.50", "true", "true", "nw-01");
    let first = import_monarch_csv(&mut kernel, &request(&held, revision)).expect("hold");
    assert_eq!(first.inserted, 1);
    assert!(!first.noop);
    assert_eq!(kernel.posting_count().expect("postings"), 0);
    let fact = kernel
        .observation_by_external_id(FEED, "nw-01")
        .expect("read")
        .expect("row");
    hint(&fact.payload_json, "\"pending_hint\":true");
    hint(&fact.payload_json, "\"pending_authoritative\":false");
    hint(&fact.payload_json, "\"reviewed_authoritative\":false");
    assert_eq!(
        kernel
            .observation_review_state(&fact.id)
            .expect("review")
            .as_deref(),
        Some("unreviewed")
    );

    let posted = tiny("-12.50", "false", "true", "nw-01");
    let second = import_monarch_csv(&mut kernel, &request(&posted, first.seq)).expect("post");
    assert_eq!(second.inserted, 0);
    assert_eq!(second.revised, 1);
    let current = kernel
        .observation_by_external_id(FEED, "nw-01")
        .expect("read")
        .expect("current");
    hint(&current.payload_json, "\"pending_hint\":false");
    let base = kernel
        .observation_base_row_hash(&current.id)
        .expect("base")
        .expect("hash");
    assert_ne!(base, current.row_hash);
    assert_eq!(
        kernel
            .observation_revision_count(&current.id)
            .expect("revs"),
        1
    );
    assert_eq!(kernel.observation_count().expect("count"), 1);
    assert_eq!(kernel.posting_count().expect("postings"), 0);
}

#[test]
fn a_changed_export_appends_a_revision_and_keeps_the_first_snapshot() {
    let tmp = Tmp::new("change");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let original = format!(
        "# synthetic\n{HEADER}\n2025-10-02,Northwind Market,Groceries,Checking ...1234,POS NORTHWIND,,-12.50,e-household,Casey Sample,true,2025-10-02 10:00:00,Rowan Checker,false,nw-01\n2025-10-03,Harbor Books,Shopping,Checking ...1234,POS HARBOR,,-3.00,,Casey Sample,false,2025-10-03 11:00:00,Rowan Checker,false,nw-02\n"
    );
    let first = import_monarch_csv(&mut kernel, &request(&original, revision)).expect("first");
    assert_eq!(first.inserted, 2);
    let changed = original.replace("-12.50", "-13.50");
    let second = import_monarch_csv(&mut kernel, &request(&changed, first.seq)).expect("second");
    assert_eq!(second.inserted, 0);
    assert_eq!(second.revised, 1);
    assert_eq!(second.unchanged, 1);
    let current = kernel
        .observation_by_external_id(FEED, "nw-01")
        .expect("read")
        .expect("current");
    assert_eq!(current.amount_minor, 1350);
    let base = kernel
        .observation_base_row_hash(&current.id)
        .expect("base")
        .expect("hash");
    assert_ne!(base, current.row_hash);
    assert_eq!(
        kernel
            .observation_revision_count(&current.id)
            .expect("revs"),
        1
    );
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
    let original = tiny("-12.50", "false", "true", "nw-01");
    let first = import_monarch_csv(&mut kernel, &request(&original, revision)).expect("first");
    let both = format!(
        "{original}2025-10-04,Harbor Books,Shopping,Checking ...1234,POS HARBOR,,-3.00,e-household,Casey Sample,true,2025-10-04 11:00:00,Rowan Checker,false,nw-02\n"
    )
    .replace("-12.50", "-9.00");
    let second = import_monarch_csv(&mut kernel, &request(&both, first.seq)).expect("second");
    assert_eq!(second.inserted, 1);
    assert_eq!(second.revised, 1);
    assert_eq!(
        kernel
            .observation_by_external_id(FEED, "nw-01")
            .expect("read")
            .expect("old")
            .amount_minor,
        900
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
    let bad_amount = tiny("nope", "false", "true", "nw-01");
    let err = import_monarch_csv(&mut kernel, &request(&bad_amount, revision)).expect_err("amount");
    let shown = err.to_string();
    assert_eq!(shown, "row=3 column=Amount");
    assert!(!shown.contains("nope"));
    assert!(!shown.contains("Northwind"));
    assert_eq!(kernel.observation_count().expect("count"), 0);
    assert_eq!(kernel.revision().expect("rev"), revision);

    let bad_account = "# synthetic\n".to_string()
        + HEADER
        + "\n2025-10-02,Northwind Market,Groceries,Mystery Checking ...9999,POS,,-1.00,e-household,Casey Sample,true,2025-10-02 10:00:00,Rowan Checker,false,nw-09\n";
    let err =
        import_monarch_csv(&mut kernel, &request(&bad_account, revision)).expect_err("account");
    let shown = err.to_string();
    assert_eq!(
        shown,
        "row=3 column=Account: map this account to an existing financial account or create one"
    );
    assert!(!shown.contains("Mystery"));
    assert!(!shown.contains("9999"));
    assert!(!shown.contains("Northwind"));

    let bad_header = "# synthetic\nDate,Merchant,Nope,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id\n";
    let err = import_monarch_csv(&mut kernel, &request(bad_header, revision)).expect_err("header");
    let shown = err.to_string();
    assert_eq!(shown, "row=2 column=Category");
    assert!(!shown.contains("Nope"));
}

#[test]
fn duplicate_ids_in_one_file_are_rejected() {
    let tmp = Tmp::new("dup");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let csv = format!("{}{}", tiny("-1.00", "false", "true", "nw-01"), "2025-10-03,Harbor Books,Shopping,Checking ...1234,POS,,-2.00,e-household,Casey Sample,true,2025-10-03 10:00:00,Rowan Checker,false,nw-01\n");
    let err = import_monarch_csv(&mut kernel, &request(&csv, revision)).expect_err("dup");
    assert_eq!(err.to_string(), "row=4 column=Id");
    assert_eq!(kernel.observation_count().expect("count"), 0);
}

#[test]
fn link_and_void_use_the_imported_observation() {
    let tmp = Tmp::new("void");
    let mut kernel = open(tmp.path());
    let (revision, cash) = setup_checking(&mut kernel);
    let csv = tiny("-12.50", "false", "true", "nw-01");
    let imported = import_monarch_csv(&mut kernel, &request(&csv, revision)).expect("import");
    let observation = imported.observation_ids[0].clone();
    let debit = "01ARZ3NDEKTSV4RRFFQ69GDBT1";
    let credit = "01ARZ3NDEKTSV4RRFFQ69GCDT1";
    let mut tx = kernel.begin_transaction(Some(imported.seq)).expect("begin");
    let groceries = tx
        .create_ledger_account(NewLedgerAccount {
            id: None,
            financial_account_id: None,
            account_type: AccountType::Expense,
            kind: LedgerKind::Category,
            parent_id: None,
            name: "Groceries".to_string(),
        })
        .expect("category");
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
                    ledger_account_id: groceries,
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

#[test]
fn a_wide_pull_voids_a_missing_row_and_a_narrow_pull_does_not() {
    let tmp = Tmp::new("window");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let first_csv = format!(
        "# synthetic\n{HEADER}\n2025-10-01,Northwind Market,Groceries,Checking ...1234,POS,,-1.00,e-household,Casey Sample,false,2025-10-01 10:00:00,Rowan Checker,false,early\n2025-10-15,Northwind Market,Groceries,Checking ...1234,POS,,-2.00,e-household,Casey Sample,false,2025-10-15 10:00:00,Rowan Checker,false,mid\n2025-10-31,Northwind Market,Groceries,Checking ...1234,POS,,-3.00,e-household,Casey Sample,false,2025-10-31 10:00:00,Rowan Checker,false,late\n"
    );
    let first = import_monarch_csv(&mut kernel, &request(&first_csv, revision)).expect("first");
    assert_eq!(first.inserted, 3);
    assert_eq!(first.voided, 0);

    let narrow = format!(
        "# synthetic\n{HEADER}\n2025-10-02,Harbor Books,Shopping,Checking ...1234,POS,,-4.00,e-household,Casey Sample,false,2025-10-02 11:00:00,Rowan Checker,false,other\n"
    );
    let second = import_monarch_csv(&mut kernel, &request(&narrow, first.seq)).expect("narrow");
    assert_eq!(second.voided, 0);
    assert_eq!(second.inserted, 1);
    assert!(kernel
        .observation_review_state(
            &kernel
                .observation_by_external_id(FEED, "mid")
                .expect("read")
                .expect("mid")
                .id
        )
        .expect("review")
        .is_some());

    let wide = format!(
        "# synthetic\n{HEADER}\n2025-10-01,Harbor Books,Shopping,Checking ...1234,POS,,-5.00,e-household,Casey Sample,false,2025-10-01 10:00:00,Rowan Checker,false,edge\n2025-10-31,Northwind Market,Groceries,Checking ...1234,POS,,-3.00,e-household,Casey Sample,false,2025-10-31 10:00:00,Rowan Checker,false,late\n"
    );
    let third = import_monarch_csv(&mut kernel, &request(&wide, second.seq)).expect("wide");
    assert_eq!(third.voided, 1);
    let mid = kernel
        .observation_by_external_id(FEED, "mid")
        .expect("read")
        .expect("mid still stored");
    assert_eq!(
        kernel.observation_review_state(&mid.id).expect("review"),
        None
    );
    let early = kernel
        .observation_by_external_id(FEED, "early")
        .expect("read")
        .expect("early");
    assert_eq!(
        kernel
            .observation_review_state(&early.id)
            .expect("review")
            .as_deref(),
        Some("unreviewed")
    );
    let late = kernel
        .observation_by_external_id(FEED, "late")
        .expect("read")
        .expect("late");
    assert_eq!(late.amount_minor, 300);
}

#[test]
fn an_unresolved_account_fails_the_whole_import() {
    let tmp = Tmp::new("unmap");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let csv = format!(
        "# synthetic\n{HEADER}\n2025-10-02,Northwind Market,Groceries,Checking ...1234,POS,,-1.00,e-household,Casey Sample,true,2025-10-02 10:00:00,Rowan Checker,false,ok-01\n2025-10-03,Harbor Books,Shopping,Mystery Checking ...9999,POS,,-2.00,e-household,Casey Sample,true,2025-10-03 10:00:00,Rowan Checker,false,bad-01\n"
    );
    let err = import_monarch_csv(&mut kernel, &request(&csv, revision)).expect_err("unmap");
    let shown = err.to_string();
    assert_eq!(
        shown,
        "row=4 column=Account: map this account to an existing financial account or create one"
    );
    assert!(!shown.contains("9999"));
    assert!(!shown.contains("Harbor"));
    assert!(!shown.contains("Northwind"));
    assert_eq!(kernel.observation_count().expect("count"), 0);
    assert_eq!(kernel.revision().expect("rev"), revision);
}

#[test]
fn a_parenthetical_last4_and_the_reviewed_status_word_import() {
    let tmp = Tmp::new("paren");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let csv = "# synthetic\n".to_string()
        + HEADER
        + "\n2025-10-02,Northwind Market,Groceries,Harbor Checking (...1234),POS,,-1.00,e-household,Casey Sample,Reviewed,2025-10-02 10:00:00,Rowan Checker,false,nw-01\n2025-10-03,Harbor Books,Shopping,Checking ...1234,POS,,-2.00,e-household,Casey Sample,,2025-10-03 10:00:00,Rowan Checker,false,nw-02\n";
    let imported = import_monarch_csv(&mut kernel, &request(&csv, revision)).expect("import");
    assert_eq!(imported.inserted, 2);
    let reviewed = kernel
        .observation_by_external_id(FEED, "nw-01")
        .expect("read")
        .expect("row");
    hint(&reviewed.payload_json, "\"reviewed_hint\":true");
    assert!(reviewed.financial_account_id.is_some());
    let blank = kernel
        .observation_by_external_id(FEED, "nw-02")
        .expect("read")
        .expect("blank");
    hint(&blank.payload_json, "\"reviewed_hint\":false");

    let bad = "# synthetic\n".to_string()
        + HEADER
        + "\n2025-10-04,Northwind Market,Groceries,Checking ...1234,POS,,-1.00,e-household,Casey Sample,Nope,,Rowan Checker,false,nw-03\n";
    let err = import_monarch_csv(&mut kernel, &request(&bad, imported.seq)).expect_err("word");
    let shown = err.to_string();
    assert_eq!(shown, "row=3 column=Reviewed");
    assert!(!shown.contains("Nope"));
    assert_eq!(kernel.observation_count().expect("count"), 2);
}

#[test]
fn payroll_without_a_mapping_fails_the_whole_import() {
    let tmp = Tmp::new("payroll-unmap");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let csv = "# synthetic\n".to_string()
        + HEADER
        + "\n2025-10-02,Northwind Market,Groceries,Checking ...1234,POS,,-1.00,e-household,Casey Sample,Reviewed,,Rowan Checker,false,ok-01\n2025-10-03,Payroll Remainder (fictional),Paychecks,Payroll Tracking,PAYROLL,,800.00,e-acme,Casey Sample,Reviewed,,Rowan Checker,false,pr-01\n";
    let mut bare = request(&csv, revision);
    bare.payroll_cross_check_name = None;
    let err = import_monarch_csv(&mut kernel, &bare).expect_err("unmap");
    let shown = err.to_string();
    assert_eq!(
        shown,
        "row=4 column=Account: map this account to an existing financial account or create one"
    );
    assert!(!shown.contains("Payroll"));
    assert!(!shown.contains("800"));
    assert_eq!(kernel.observation_count().expect("count"), 0);
}

#[test]
fn a_mapped_payroll_account_is_stored_and_not_posted() {
    let tmp = Tmp::new("payroll");
    let mut kernel = open(tmp.path());
    let revision = setup_checking(&mut kernel).0;
    let csv = "# synthetic\n".to_string()
        + HEADER
        + "\n2025-10-03,Payroll Remainder (fictional),Paychecks,Payroll Tracking,PAYROLL,,800.00,e-acme,Casey Sample,Reviewed,,Rowan Checker,false,pr-pos\n2025-10-03,Payroll Tax (fictional),Taxes,Payroll Tracking,PAYROLL,,-800.00,e-acme,Casey Sample,Reviewed,,Rowan Checker,false,pr-tax\n";
    let imported = import_monarch_csv(&mut kernel, &request(&csv, revision)).expect("import");
    assert_eq!(imported.inserted, 2);
    assert_eq!(kernel.posting_count().expect("postings"), 0);
    let row = kernel
        .observation_by_external_id(FEED, "pr-pos")
        .expect("read")
        .expect("row");
    assert!(row.financial_account_id.is_none());
    assert!(row.cross_check_account_id.is_some());
    hint(&row.payload_json, "\"cross_check_role\":\"payroll\"");
    let again = import_monarch_csv(&mut kernel, &request(&csv, imported.seq)).expect("again");
    assert!(again.noop);
    assert_eq!(kernel.observation_count().expect("count"), 2);
}
