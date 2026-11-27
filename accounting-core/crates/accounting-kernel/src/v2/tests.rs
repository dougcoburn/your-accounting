//! Synthetic U17 fixtures. Amounts and names are examples, not records.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;

use super::error::persists_freeze;
use super::sqlmap::{map_message, SqlSite};
use super::types::is_ulid;
use super::*;

const WHEN: &str = "2026-01-02T03:04:05Z";
const MINOR: i64 = 1500;

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
            std::env::temp_dir().join(format!("ya-u17-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self { dir }
    }

    fn db(&self) -> PathBuf {
        self.dir.join("entity.sqlite")
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct TickClock {
    n: Rc<Cell<u32>>,
}

impl Clock for TickClock {
    fn now_utc(&mut self) -> String {
        let n = self.n.get() + 1;
        self.n.set(n);
        format!("2026-01-02T03:04:{n:02}Z")
    }
}

struct FixedIds;

impl IdSource for FixedIds {
    fn next_id(&mut self) -> String {
        "not-a-ulid".to_string()
    }
}

struct Books {
    entity: String,
    employer: String,
    card_fin: String,
    card: String,
    food: String,
    groceries: String,
}

fn uid(suffix: &str) -> String {
    let id = format!("01ARZ3NDEKTSV4RRFFQ69G{suffix}");
    assert!(is_ulid(&id), "fixture suffix {suffix}");
    id
}

fn clock() -> Box<dyn Clock> {
    Box::new(FixedClock {
        timestamp: WHEN.to_string(),
    })
}

fn open_at(path: &Path) -> Kernel {
    Kernel::open(path, clock(), Box::new(SequenceIds::new()), "u17").expect("open")
}

fn code_of(err: Error, code: Code) -> Error {
    assert_eq!(err.code, code, "{err}");
    assert_eq!(err.class, code.class(), "{err}");
    err
}

fn party(id: &str, name: &str, label: PartyLabel) -> NewParty {
    NewParty {
        id: Some(id.to_string()),
        name: name.to_string(),
        label,
        parent_id: None,
    }
}

fn financial(
    id: &str,
    name: &str,
    kind: FinancialKind,
    currency: &str,
    last4: Option<&str>,
) -> NewFinancialAccount {
    NewFinancialAccount {
        id: Some(id.to_string()),
        name: name.to_string(),
        kind,
        currency: currency.to_string(),
        tracking_mode: TrackingMode::Transparent,
        institution: None,
        last4: last4.map(str::to_string),
    }
}

fn ledger(
    id: &str,
    financial_id: Option<&str>,
    account_type: AccountType,
    kind: LedgerKind,
    parent: Option<&str>,
    name: &str,
) -> NewLedgerAccount {
    NewLedgerAccount {
        id: Some(id.to_string()),
        financial_account_id: financial_id.map(str::to_string),
        account_type,
        kind,
        parent_id: parent.map(str::to_string),
        name: name.to_string(),
    }
}

fn posting(id: &str, account: &str, side: Side, amount: i64, currency: &str) -> NewPosting {
    NewPosting {
        id: Some(uid(id)),
        ledger_account_id: account.to_string(),
        currency: currency.to_string(),
        side,
        amount_minor: amount,
        party_id: None,
    }
}

fn event(id: &str, postings: Vec<NewPosting>) -> NewEvent {
    NewEvent {
        id: Some(uid(id)),
        recipe_id: "u17_probe".to_string(),
        recipe_version: "1".to_string(),
        effective_date: "2026-01-02".to_string(),
        postings,
        recipe_instance_id: None,
        counterpart_entity_id: None,
        expectations: Vec::new(),
    }
}

fn recipe_half(
    books: &Books,
    event_id: &str,
    debit: &str,
    credit: &str,
    amount: i64,
    expected: i64,
) -> NewEvent {
    let mut new = event(
        event_id,
        vec![
            posting(debit, &books.groceries, Side::Debit, amount, "USD"),
            posting(credit, &books.card, Side::Credit, amount, "USD"),
        ],
    );
    new.recipe_instance_id = Some(uid("RR01"));
    new.counterpart_entity_id = Some(uid("XENT"));
    new.expectations = vec![CounterpartExpectation {
        currency: "USD".to_string(),
        expected_amount_minor: expected,
    }];
    new
}

fn purchase(
    books: &Books,
    debit_id: &str,
    credit_id: &str,
    debit_acct: &str,
    amount: i64,
    currency: &str,
) -> NewEvent {
    event(
        "EV01",
        vec![
            posting(debit_id, debit_acct, Side::Debit, amount, currency),
            posting(credit_id, &books.card, Side::Credit, amount, currency),
        ],
    )
}

fn chart(kernel: &mut Kernel) -> Books {
    let books = Books {
        entity: uid("ENT1"),
        employer: uid("ACME"),
        card_fin: uid("CARD"),
        card: uid("CRD1"),
        food: uid("FD01"),
        groceries: uid("GRC1"),
    };
    let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
    tx.create_entity(NewEntity {
        id: Some(books.entity.clone()),
        party_id: Some(uid("EPT1")),
        name: "Household".to_string(),
    })
    .expect("entity");
    tx.create_party(party(&books.employer, "Acme Corp", PartyLabel::Employer))
        .expect("party");
    tx.add_party_alias(&books.employer, "Example Grocer")
        .expect("alias");
    tx.rename(ReferenceKind::Party, &books.employer, "Acme Corp Payroll")
        .expect("rename");
    tx.create_financial_account(financial(
        &books.card_fin,
        "Example Card",
        FinancialKind::CreditCard,
        "USD",
        Some("4242"),
    ))
    .expect("financial");
    tx.create_ledger_account(ledger(
        &books.card,
        Some(&books.card_fin),
        AccountType::Liability,
        LedgerKind::Card,
        None,
        "Example Card",
    ))
    .expect("card");
    tx.create_ledger_account(ledger(
        &books.food,
        None,
        AccountType::Expense,
        LedgerKind::Category,
        None,
        "Food",
    ))
    .expect("food");
    tx.create_ledger_account(ledger(
        &books.groceries,
        None,
        AccountType::Expense,
        LedgerKind::Category,
        Some(&books.food),
        "Groceries",
    ))
    .expect("groceries");
    tx.register_recipe("u17_probe", "1").expect("recipe");
    let sealed = tx.commit().expect("commit");
    assert!(!sealed.noop);
    assert_eq!(sealed.seq, 1);
    books
}

fn column_names(kernel: &Kernel, table: &str) -> Vec<String> {
    let mut stmt = kernel
        .connection()
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("pragma");
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("rows");
    rows.map(|row| row.expect("column")).collect()
}

fn expectation_rows(kernel: &Kernel, event_id: &str) -> Vec<(String, i64)> {
    let mut stmt = kernel
        .connection()
        .prepare(
            "SELECT currency, expected_amount_minor
             FROM event_counterpart_expectations WHERE event_id = ?1 ORDER BY currency",
        )
        .expect("expectations");
    let rows = stmt
        .query_map([event_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("rows");
    rows.map(|row| row.expect("expectation")).collect()
}

fn event_field(kernel: &Kernel, column: &str, id: &str) -> Option<String> {
    let sql = format!("SELECT {column} FROM accounting_events WHERE id = ?1");
    kernel
        .connection()
        .query_row(&sql, [id], |row| row.get(0))
        .expect("event")
}

fn current_halves(kernel: &Kernel, instance: &str) -> i64 {
    kernel
        .connection()
        .query_row(
            "SELECT count(*) FROM accounting_events
             WHERE recipe_instance_id = ?1 AND superseded_by IS NULL",
            [instance],
            |row| row.get(0),
        )
        .expect("count")
}

#[test]
fn happy_path_typed_accounts_parties_and_one_clock_read() {
    let tmp = Tmp::new("happy");
    let ticks = Rc::new(Cell::new(0));
    let mut kernel = Kernel::open(
        tmp.db(),
        Box::new(TickClock { n: ticks.clone() }),
        Box::new(SequenceIds::new()),
        "u17",
    )
    .expect("open");
    assert!(kernel.is_writable());
    let flag: i64 = kernel
        .connection()
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("pragma");
    assert_eq!(flag, 1);

    let books = chart(&mut kernel);
    assert_eq!(ticks.get(), 1);
    assert_eq!(kernel.revision().expect("rev"), 1);

    let entity = kernel.entity().expect("read").expect("entity");
    assert_eq!(entity.id, books.entity);
    assert!(entity.name == "Household", "entity name was not stored");
    assert_eq!(entity.created_at, "2026-01-02T03:04:01Z");

    let stored = kernel.party(&books.employer).expect("read").expect("party");
    assert_eq!(stored.id, books.employer);
    assert_eq!(stored.label, "employer");
    assert!(stored.name == "Acme Corp", "party row was renamed in place");
    let current = kernel
        .current_name(ReferenceKind::Party, &books.employer)
        .expect("name")
        .expect("current");
    assert!(
        current == "Acme Corp Payroll",
        "current name did not follow the rename"
    );

    let card = kernel
        .financial_account(&books.card_fin)
        .expect("read")
        .expect("card");
    assert_eq!(card.kind, "credit_card");
    assert_eq!(card.currency, "USD");
    assert_eq!(card.last4.as_deref(), Some("4242"));
    let ledger = kernel
        .ledger_account(&books.card)
        .expect("read")
        .expect("ledger");
    assert_eq!(ledger.account_type, "liability");
    assert_eq!(ledger.kind, "card");
    assert_eq!(
        ledger.financial_account_id.as_deref(),
        Some(books.card_fin.as_str())
    );
    let leaf = kernel
        .ledger_account(&books.groceries)
        .expect("read")
        .expect("leaf");
    assert_eq!(leaf.parent_id.as_deref(), Some(books.food.as_str()));
    assert_eq!(leaf.kind, "category");

    let info = kernel.commit_at_seq(1).expect("read").expect("commit");
    assert_eq!(info.expected_revision, 0);
    assert_eq!(info.seq, 1);
    assert_eq!(info.kind, "setup");
    assert_eq!(info.recorded_at, entity.created_at);
    assert_eq!(info.id, "01ARZ3NDEKTSV4RRFFQ69G0001");
    assert_ne!(info.id, books.employer);

    let aliases: i64 = kernel
        .connection()
        .query_row(
            "SELECT count(*) FROM party_aliases WHERE party_id = ?1 AND alias = ?2",
            params![books.employer, "Example Grocer"],
            |row| row.get(0),
        )
        .expect("alias");
    assert_eq!(aliases, 1);

    let entity_cols = column_names(&kernel, "entity");
    assert!(entity_cols.iter().all(|name| name != "visibility"));
    let party_cols = column_names(&kernel, "parties");
    assert!(party_cols.iter().all(|name| name != "opaque_entity_id"));
    let ledger_cols = column_names(&kernel, "ledger_accounts");
    assert!(ledger_cols.iter().any(|name| name == "account_type"));
    assert!(ledger_cols.iter().any(|name| name == "kind"));
    assert!(ledger_cols.iter().any(|name| name == "parent_id"));
    assert!(ledger_cols
        .iter()
        .all(|name| { name != "visibility" && name != "contra" && name != "entity_id" }));
    let posting_cols = column_names(&kernel, "journal_postings");
    assert!(posting_cols.iter().all(|name| name != "entity_id"));
    assert!(party_cols.iter().all(|name| name != "entity_id"));
}

#[test]
fn caller_omitted_id_comes_from_the_injected_source() {
    let tmp = Tmp::new("ids");
    let mut kernel = open_at(&tmp.db());
    let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
    let id = tx
        .create_party(NewParty {
            id: None,
            name: "Example Grocer".to_string(),
            label: PartyLabel::Vendor,
            parent_id: None,
        })
        .expect("party");
    let sealed = tx.commit().expect("commit");
    assert_eq!(
        sealed.commit_id.as_deref(),
        Some("01ARZ3NDEKTSV4RRFFQ69G0001")
    );
    assert_eq!(id, "01ARZ3NDEKTSV4RRFFQ69G0002");
    assert!(is_ulid(&id));
}

#[test]
fn drop_without_commit_rolls_back() {
    let tmp = Tmp::new("drop");
    let mut kernel = open_at(&tmp.db());
    let id = uid("VND1");
    {
        let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
        tx.create_party(party(&id, "Example Grocer", PartyLabel::Vendor))
            .expect("party");
    }
    assert!(kernel.party(&id).expect("read").is_none());
    assert_eq!(kernel.revision().expect("rev"), 0);
    assert_eq!(kernel.commit_count().expect("count"), 0);
}

#[test]
fn revision_protocol_uses_the_caller_integer() {
    let tmp = Tmp::new("rev");
    let mut kernel = open_at(&tmp.db());

    let missing = code_of(
        kernel.begin_transaction(None).expect_err("missing"),
        Code::MissingExpectedRevision,
    );
    assert!(missing.refs.is_empty());
    let stale = code_of(
        kernel.begin_transaction(Some(-1)).expect_err("negative"),
        Code::StaleRevision,
    );
    assert_eq!(stale.refs, vec!["AT-032".to_string()]);
    let tx = kernel.begin_transaction(Some(0)).expect("lock released");
    let bare = tx.commit().expect("noop");
    assert!(bare.noop);
    assert!(bare.commit_id.is_none());
    assert!(bare.subjects.is_empty());
    assert_eq!(bare.seq, 0);
    assert_eq!(kernel.revision().expect("rev"), 0);

    let _books = chart(&mut kernel);
    for (rev, suffix) in [(1, "P001"), (2, "P002"), (3, "P003")] {
        let sealed = {
            let mut tx = kernel.begin_transaction(Some(rev)).expect("begin");
            tx.create_party(party(&uid(suffix), "Example Person", PartyLabel::Person))
                .expect("party");
            tx.commit().expect("commit")
        };
        assert_eq!(sealed.seq, rev + 1);
        let info = kernel
            .commit_at_seq(sealed.seq)
            .expect("read")
            .expect("row");
        assert_eq!(info.expected_revision, rev);
        assert_eq!(info.seq, rev + 1);
    }
    assert_eq!(kernel.revision().expect("rev"), 4);
    assert_eq!(kernel.commit_count().expect("count"), 4);
    let rejected = code_of(
        kernel.begin_transaction(Some(3)).expect_err("stale"),
        Code::StaleRevision,
    );
    assert_eq!(rejected.refs, vec!["AT-032".to_string()]);
    assert_eq!(kernel.commit_count().expect("count"), 4);
    assert!(kernel.party(&uid("P004")).expect("read").is_none());

    let mut tx = kernel.begin_transaction(Some(4)).expect("current");
    tx.create_party(party(&uid("P004"), "Example Person", PartyLabel::Person))
        .expect("party");
    let sealed = tx.commit().expect("commit");
    assert_eq!(sealed.seq, 5);
    let info = kernel.commit_at_seq(5).expect("read").expect("row");
    assert_eq!(info.expected_revision, 4);
    assert_eq!(info.seq, 5);
    assert_eq!(info.kind, "setup");
}

#[test]
fn foreign_keys_off_refuses_writes_and_reads_still_work() {
    let tmp = Tmp::new("fk");
    let mut kernel = Kernel::open_observing_disabled_foreign_keys(
        tmp.db(),
        clock(),
        Box::new(SequenceIds::new()),
        "u17",
    )
    .expect("open");
    assert!(!kernel.is_writable());
    let flag: i64 = kernel
        .connection()
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("pragma");
    assert_eq!(flag, 0);
    assert_eq!(kernel.revision().expect("read"), 0);
    let begin = code_of(
        kernel.begin_transaction(Some(0)).expect_err("begin"),
        Code::ForeignKeysDisabled,
    );
    assert_eq!(begin.class, Class::Bugcheck);
    assert_eq!(begin.refs, vec!["PR-19".to_string()]);
    let freeze = code_of(
        kernel
            .record_write_freeze(Some(0), "UnsealedRows")
            .expect_err("freeze"),
        Code::ForeignKeysDisabled,
    );
    assert_eq!(freeze.class, Class::Bugcheck);
    assert_eq!(kernel.kernel_state_count().expect("count"), 0);
    assert_eq!(kernel.commit_count().expect("count"), 0);
    assert!(kernel.entity().expect("read").is_none());
}

#[test]
fn open_failures_do_not_echo_paths_or_bad_ids() {
    let tmp = Tmp::new("open");
    std::fs::create_dir(tmp.db()).expect("dir");
    let err = code_of(
        Kernel::open(tmp.db(), clock(), Box::new(SequenceIds::new()), "u17").expect_err("dir"),
        Code::StorageFault,
    );
    assert_eq!(err.class, Class::Bugcheck);
    assert!(
        !err.to_string().contains("ya-u17"),
        "storage fault leaked a path"
    );

    let shape = code_of(
        Kernel::open(
            tmp.dir.join("missing.sqlite"),
            clock(),
            Box::new(SequenceIds::new()),
            "",
        )
        .expect_err("actor"),
        Code::InvalidShape,
    );
    assert!(shape.token.is_none());

    let kernel = open_at(&tmp.dir.join("ids.sqlite"));
    let err = code_of(
        Kernel::open(
            tmp.dir.join("bad-ids.sqlite"),
            clock(),
            Box::new(FixedIds),
            "u17",
        )
        .and_then(|mut kernel| kernel.begin_transaction(Some(0)).map(|_| ()))
        .expect_err("ids"),
        Code::InvalidId,
    );
    assert!(
        !err.to_string().contains("not-a-ulid"),
        "invalid id was echoed"
    );
    assert_eq!(kernel.revision().expect("rev"), 0);
    let _ = kernel;
}

#[test]
fn write_lock_is_write_locked() {
    let tmp = Tmp::new("lock");
    let mut held = open_at(&tmp.db());
    let mut other = open_at(&tmp.db());
    let tx = held.begin_transaction(Some(0)).expect("hold");
    let err = code_of(
        other.begin_transaction(Some(0)).expect_err("busy"),
        Code::WriteLocked,
    );
    assert_eq!(err.class, Class::User);
    drop(tx);
    let tx = other.begin_transaction(Some(0)).expect("released");
    drop(tx);
}

#[test]
fn freeze_blocks_writes_until_clear() {
    let tmp = Tmp::new("freeze");
    let mut kernel = open_at(&tmp.db());
    let _books = chart(&mut kernel);

    let stale = code_of(
        kernel
            .record_write_freeze(Some(0), "UnsealedRows")
            .expect_err("stale"),
        Code::StaleRevision,
    );
    assert_eq!(stale.refs, vec!["AT-032".to_string()]);
    assert_eq!(kernel.kernel_state_count().expect("count"), 0);

    let bad = code_of(
        kernel
            .record_write_freeze(Some(1), "NotACode")
            .expect_err("code"),
        Code::InvalidShape,
    );
    assert!(bad.token.is_none());
    assert_eq!(bad.ids, vec!["NotACode".to_string()]);
    assert_eq!(kernel.kernel_state_count().expect("count"), 0);
    assert_eq!(kernel.revision().expect("rev"), 1);

    let missing = code_of(
        kernel
            .record_write_freeze(None, "UnsealedRows")
            .expect_err("missing"),
        Code::MissingExpectedRevision,
    );
    assert!(missing.ids.is_empty());

    let frozen = kernel
        .record_write_freeze(Some(1), "UnsealedRows")
        .expect("freeze");
    assert!(!frozen.already_present);
    assert_eq!(frozen.seq, 2);
    assert_eq!(frozen.code, "UnsealedRows");
    let info = kernel.commit_at_seq(2).expect("read").expect("commit");
    assert_eq!(info.kind, "freeze");
    assert_eq!(info.expected_revision, 1);
    assert_eq!(kernel.kernel_state_count().expect("count"), 1);

    let blocked = code_of(
        kernel.begin_transaction(Some(2)).expect_err("frozen"),
        Code::WriteFrozen,
    );
    assert_eq!(blocked.class, Class::Bugcheck);
    assert_eq!(blocked.refs, vec!["UnsealedRows".to_string()]);
    assert!(blocked.ids.contains(&frozen.row_id));
    assert!(blocked.ids.contains(&frozen.commit_id));
    let earlier = code_of(
        kernel.begin_transaction(Some(0)).expect_err("order"),
        Code::WriteFrozen,
    );
    assert_eq!(earlier.refs, vec!["UnsealedRows".to_string()]);

    let again = kernel
        .record_write_freeze(Some(2), "IntegrityFailed")
        .expect("again");
    assert!(again.already_present);
    assert_eq!(again.commit_id, frozen.commit_id);
    assert_eq!(again.row_id, frozen.row_id);
    assert_eq!(kernel.kernel_state_count().expect("count"), 1);

    let clear_stale = code_of(
        kernel.clear_write_freeze(Some(0)).expect_err("stale"),
        Code::StaleRevision,
    );
    assert_eq!(clear_stale.class, Class::User);
    assert_eq!(kernel.revision().expect("rev"), 2);
    assert_eq!(kernel.freeze().expect("read").expect("row").seq, 2);

    let cleared = kernel.clear_write_freeze(Some(2)).expect("clear");
    assert!(!cleared.noop);
    assert_eq!(cleared.seq, 3);
    assert_eq!(
        kernel.commit_at_seq(3).expect("read").expect("row").kind,
        "freeze"
    );
    assert_eq!(kernel.kernel_state_count().expect("count"), 2);
    let live = kernel.freeze().expect("read").expect("row");
    assert_eq!(live.seq, 3);
    assert_eq!(live.code, "UnsealedRows");
    assert!(!live.already_present);

    let mut tx = kernel.begin_transaction(Some(3)).expect("resumed");
    tx.create_party(party(&uid("VND1"), "Example Grocer", PartyLabel::Vendor))
        .expect("party");
    assert_eq!(tx.commit().expect("commit").seq, 4);
}

#[test]
fn clear_without_a_freeze_writes_nothing() {
    let tmp = Tmp::new("noclear");
    let mut kernel = open_at(&tmp.db());
    let err = code_of(
        kernel.clear_write_freeze(Some(0)).expect_err("clear"),
        Code::InvalidShape,
    );
    assert!(err.token.is_none());
    assert_eq!(kernel.commit_count().expect("count"), 0);
    let missing = code_of(
        kernel.clear_write_freeze(None).expect_err("missing"),
        Code::MissingExpectedRevision,
    );
    assert!(missing.ids.is_empty());
    let tx = kernel.begin_transaction(Some(0)).expect("not locked");
    drop(tx);
}

#[test]
fn user_error_does_not_persist_a_freeze() {
    let tmp = Tmp::new("nofreeze");
    let mut kernel = open_at(&tmp.db());
    let err = {
        let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
        tx.create_party(party("not-a-ulid", "Example Grocer", PartyLabel::Vendor))
            .expect_err("id")
    };
    let err = code_of(err, Code::InvalidId);
    assert!(
        !err.to_string().contains("not-a-ulid"),
        "invalid id was echoed"
    );
    assert_eq!(kernel.kernel_state_count().expect("count"), 0);
    assert_eq!(kernel.revision().expect("rev"), 0);
    let mut tx = kernel.begin_transaction(Some(0)).expect("retry");
    tx.create_party(party(&uid("VND1"), "Example Grocer", PartyLabel::Vendor))
        .expect("party");
    tx.commit().expect("commit");
}

#[test]
fn recipe_retry_is_a_noop_and_a_different_payload_conflicts() {
    let tmp = Tmp::new("recipe");
    let mut kernel = open_at(&tmp.db());
    let books = chart(&mut kernel);
    let event_id = {
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        let event_id = tx
            .stage_event(recipe_half(&books, "EV20", "PS20", "PS21", MINOR, MINOR))
            .expect("event");
        let sealed = tx.commit().expect("commit");
        assert!(!sealed.noop);
        assert_eq!(sealed.seq, 2);
        let info = kernel.commit_at_seq(2).expect("read").expect("row");
        assert_eq!(info.kind, "cross_entity");
        assert_eq!(info.expected_revision, 1);
        assert_eq!(info.id, sealed.commit_id.expect("id"));
        event_id
    };
    assert_eq!(
        event_field(&kernel, "recipe_instance_id", &event_id),
        Some(uid("RR01"))
    );
    assert_eq!(
        event_field(&kernel, "counterpart_entity_id", &event_id),
        Some(uid("XENT"))
    );
    assert_eq!(kernel.posting_count().expect("count"), 2);
    let original = kernel.commit_at_seq(2).expect("read").expect("row").id;

    let retry = {
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        let returned = tx
            .stage_event(recipe_half(&books, "EV21", "PS22", "PS23", MINOR, MINOR))
            .expect("retry");
        assert_eq!(returned, event_id);
        tx.commit().expect("noop")
    };
    assert!(retry.noop);
    assert!(retry.commit_id.is_none());
    assert_eq!(retry.seq, 2);
    match retry.subjects.as_slice() {
        [Subject::RecipeInstance {
            recipe_instance_id,
            event_id: stored,
            commit_id,
            seq,
        }] => {
            assert_eq!(recipe_instance_id, &uid("RR01"));
            assert_eq!(stored, &event_id);
            assert_eq!(commit_id, &original);
            assert_eq!(*seq, 2);
        }
        _ => panic!("noop subject"),
    }
    assert_eq!(kernel.revision().expect("rev"), 2);
    assert_eq!(current_halves(&kernel, &uid("RR01")), 1);
    assert_eq!(kernel.commit_count().expect("count"), 2);

    let conflict = {
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        tx.create_party(party(&uid("VND2"), "Example Grocer", PartyLabel::Vendor))
            .expect("party");
        tx.stage_event(recipe_half(
            &books,
            "EV22",
            "PS24",
            "PS25",
            MINOR,
            MINOR + 1,
        ))
        .expect_err("conflict")
    };
    let conflict = code_of(conflict, Code::IdempotencyConflict);
    assert_eq!(conflict.class, Class::User);
    assert_eq!(conflict.refs, vec!["ADR 0004".to_string()]);
    assert_eq!(conflict.ids, vec![uid("RR01"), event_id.clone()]);
    let text = conflict.to_string();
    assert!(
        !text.contains("1500") && !text.contains("1501") && !text.to_lowercase().contains("acme"),
        "idempotency conflict leaked a payload"
    );
    assert!(
        kernel.party(&uid("VND2")).expect("read").is_none(),
        "conflict committed a party"
    );
    assert_eq!(kernel.commit_count().expect("count"), 2);
    assert_eq!(current_halves(&kernel, &uid("RR01")), 1);

    let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
    let returned = tx
        .stage_event(recipe_half(&books, "EV21", "PS22", "PS23", MINOR, MINOR))
        .expect("retry");
    assert_eq!(returned, event_id);
    tx.create_party(party(&uid("VND1"), "Example Grocer", PartyLabel::Vendor))
        .expect("party");
    let sealed = tx.commit().expect("commit");
    assert!(!sealed.noop);
    assert_eq!(sealed.seq, 3);
    assert_eq!(
        kernel.commit_at_seq(3).expect("read").expect("row").kind,
        "setup"
    );
    assert_eq!(current_halves(&kernel, &uid("RR01")), 1);
    assert!(kernel.party(&uid("VND1")).expect("read").is_some());
}

#[test]
fn recipe_fields_are_all_or_none_and_the_counterpart_is_not_self() {
    let tmp = Tmp::new("recipe-shape");
    let mut kernel = open_at(&tmp.db());
    let books = chart(&mut kernel);

    let partial = {
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        let mut new = recipe_half(&books, "EV20", "PS20", "PS21", MINOR, MINOR);
        new.counterpart_entity_id = None;
        tx.stage_event(new).expect_err("partial")
    };
    assert_eq!(
        code_of(partial, Code::InvalidShape).token,
        Some("recipe_fields")
    );
    assert_eq!(kernel.posting_count().expect("count"), 0);

    let itself = {
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        let mut new = recipe_half(&books, "EV20", "PS20", "PS21", MINOR, MINOR);
        new.counterpart_entity_id = Some(books.entity.clone());
        tx.stage_event(new).expect_err("self")
    };
    let itself = code_of(itself, Code::InvalidShape);
    assert_eq!(itself.token, Some("counterpart_self"));
    assert!(
        !itself.to_string().contains("Household"),
        "counterpart check leaked the entity name"
    );
    assert_eq!(kernel.revision().expect("rev"), 1);
    assert_eq!(kernel.commit_count().expect("count"), 1);
}

#[test]
fn recipe_expectations_are_a_set_and_intercompany_postings_name_a_party() {
    let tmp = Tmp::new("multi");
    let mut kernel = open_at(&tmp.db());
    let books = chart(&mut kernel);
    let equity = uid("EQ01");
    let receivable = uid("AR01");
    let payable = uid("AP01");
    {
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        tx.create_ledger_account(ledger(
            &equity,
            None,
            AccountType::Equity,
            LedgerKind::Equity,
            None,
            "Opening",
        ))
        .expect("equity");
        tx.create_ledger_account(ledger(
            &receivable,
            None,
            AccountType::Asset,
            LedgerKind::Receivable,
            None,
            "Due from counterpart",
        ))
        .expect("receivable");
        tx.create_ledger_account(ledger(
            &payable,
            None,
            AccountType::Liability,
            LedgerKind::Payable,
            None,
            "Due to counterpart",
        ))
        .expect("payable");
        tx.commit().expect("accounts");
    }

    let half = |event_id: &str,
                usd_debit: &str,
                usd_credit: &str,
                eur_debit: &str,
                eur_credit: &str,
                rows: Vec<(&str, i64)>| {
        let mut new = event(
            event_id,
            vec![
                posting(usd_debit, &books.groceries, Side::Debit, 500, "USD"),
                posting(usd_credit, &equity, Side::Credit, 500, "USD"),
                posting(eur_debit, &books.groceries, Side::Debit, 300, "EUR"),
                posting(eur_credit, &equity, Side::Credit, 300, "EUR"),
            ],
        );
        new.recipe_instance_id = Some(uid("RR02"));
        new.counterpart_entity_id = Some(uid("XENT"));
        new.expectations = rows
            .into_iter()
            .map(|(currency, amount)| CounterpartExpectation {
                currency: currency.to_string(),
                expected_amount_minor: amount,
            })
            .collect();
        new
    };

    let event_id = {
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        let id = tx
            .stage_event(half(
                "EV40",
                "PS40",
                "PS41",
                "PS4M",
                "PS4N",
                vec![("EUR", 300), ("USD", 500)],
            ))
            .expect("two currencies");
        let sealed = tx.commit().expect("commit");
        assert_eq!(sealed.seq, 3);
        assert_eq!(
            kernel.commit_at_seq(3).expect("read").expect("row").kind,
            "cross_entity"
        );
        id
    };
    assert_eq!(
        expectation_rows(&kernel, &event_id),
        vec![("EUR".to_string(), 300), ("USD".to_string(), 500)]
    );

    let retry = {
        let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
        let returned = tx
            .stage_event(half(
                "EV41",
                "PS42",
                "PS43",
                "PS4P",
                "PS4Q",
                vec![("USD", 500), ("EUR", 300)],
            ))
            .expect("order does not matter");
        assert_eq!(returned, event_id);
        tx.commit().expect("noop")
    };
    assert!(retry.noop);
    assert_eq!(current_halves(&kernel, &uid("RR02")), 1);

    let changed = {
        let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
        tx.stage_event(half(
            "EV42",
            "PS44",
            "PS45",
            "PS4R",
            "PS4S",
            vec![("USD", 500), ("EUR", 301)],
        ))
        .expect_err("different rows")
    };
    let changed = code_of(changed, Code::IdempotencyConflict);
    assert_eq!(changed.ids, vec![uid("RR02"), event_id.clone()]);
    assert!(
        !changed.to_string().contains("301"),
        "conflict leaked an expected amount"
    );
    assert_eq!(
        expectation_rows(&kernel, &event_id),
        vec![("EUR".to_string(), 300), ("USD".to_string(), 500)]
    );

    let mut reject = |new: NewEvent, code: Code| {
        let err = {
            let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
            tx.stage_event(new).expect_err("shape")
        };
        assert_eq!(kernel.revision().expect("rev"), 3);
        code_of(err, code)
    };

    let mut duplicate = half(
        "EV43",
        "PS46",
        "PS47",
        "PS4T",
        "PS4V",
        vec![("USD", 500), ("USD", 200)],
    );
    duplicate.postings.truncate(2);
    assert_eq!(
        reject(duplicate, Code::InvalidShape).token,
        Some("counterpart_expectations")
    );

    let mut empty = half("EV44", "PS48", "PS49", "PS4W", "PS4X", vec![]);
    empty.postings.truncate(2);
    assert_eq!(
        reject(empty, Code::InvalidShape).token,
        Some("counterpart_expectations")
    );

    let mut bad_currency = half("EV45", "PS4A", "PS4B", "PS4Y", "PS4Z", vec![("usd", 500)]);
    bad_currency.postings.truncate(2);
    assert_eq!(
        reject(bad_currency, Code::InvalidShape).token,
        Some("counterpart_expectations")
    );

    let mut non_positive = half("EV46", "PS4C", "PS4D", "PS50", "PS51", vec![("USD", 0)]);
    non_positive.postings.truncate(2);
    assert_eq!(
        reject(non_positive, Code::InvalidMoney).code,
        Code::InvalidMoney
    );

    let mut plain = event(
        "EV47",
        vec![
            posting("PS4E", &books.groceries, Side::Debit, MINOR, "USD"),
            posting("PS4F", &books.card, Side::Credit, MINOR, "USD"),
        ],
    );
    plain.expectations = vec![CounterpartExpectation {
        currency: "USD".to_string(),
        expected_amount_minor: MINOR,
    }];
    assert_eq!(
        reject(plain, Code::InvalidShape).token,
        Some("counterpart_expectations")
    );

    let mut missing_party = recipe_half(&books, "EV48", "PS4G", "PS4H", MINOR, MINOR);
    missing_party.recipe_instance_id = Some(uid("RR06"));
    missing_party.postings[0].ledger_account_id = receivable.clone();
    let missing_party = reject(missing_party, Code::InvalidShape);
    assert_eq!(missing_party.token, Some("recipe_party"));
    assert_eq!(missing_party.refs, vec!["ADR 0004".to_string()]);
    assert_eq!(missing_party.ids, vec![receivable.clone()]);

    let mut missing_payable = recipe_half(&books, "EV4A", "PS4H", "PS4J", MINOR, MINOR);
    missing_payable.recipe_instance_id = Some(uid("RR07"));
    missing_payable.postings[1].ledger_account_id = payable.clone();
    let missing_payable = reject(missing_payable, Code::InvalidShape);
    assert_eq!(missing_payable.token, Some("recipe_party"));
    assert_eq!(missing_payable.refs, vec!["ADR 0004".to_string()]);
    assert_eq!(missing_payable.ids, vec![payable]);

    let mut named = recipe_half(&books, "EV49", "PS4K", "PS52", MINOR, MINOR);
    named.recipe_instance_id = Some(uid("RR06"));
    named.postings[0].ledger_account_id = receivable.clone();
    named.postings[0].party_id = Some(books.employer.clone());
    let named_id = {
        let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
        let id = tx.stage_event(named).expect("party set");
        let sealed = tx.commit().expect("commit");
        assert_eq!(sealed.seq, 4);
        id
    };

    // A corrected replacement is also a half, so a dropped party is the same token.
    let mut dropped = recipe_half(&books, "EV4B", "PS53", "PS54", MINOR, MINOR);
    dropped.recipe_instance_id = Some(uid("RR06"));
    dropped.postings[0].ledger_account_id = receivable.clone();
    let dropped_party = {
        let mut tx = kernel.begin_transaction(Some(4)).expect("begin");
        tx.correct_event(&named_id, Some(dropped))
            .expect_err("replacement party")
    };
    let dropped_party = code_of(dropped_party, Code::InvalidShape);
    assert_eq!(dropped_party.token, Some("recipe_party"));
    assert_eq!(dropped_party.refs, vec!["ADR 0004".to_string()]);
    assert_eq!(dropped_party.ids, vec![receivable]);
    assert_eq!(kernel.revision().expect("rev"), 4);
    assert_eq!(current_halves(&kernel, &uid("RR06")), 1);
}

#[test]
fn correction_supersedes_before_the_replacement_and_a_retired_instance_conflicts() {
    let tmp = Tmp::new("correct");
    let mut kernel = open_at(&tmp.db());
    let books = chart(&mut kernel);
    let original = {
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        let id = tx
            .stage_event(recipe_half(&books, "EV20", "PS20", "PS21", MINOR, MINOR))
            .expect("half");
        tx.commit().expect("commit");
        id
    };

    let replacement_id = {
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        let next = recipe_half(&books, "EV23", "PS30", "PS31", MINOR - 100, MINOR - 100);
        let id = tx
            .correct_event(&original, Some(next))
            .expect("correct")
            .expect("replacement");
        let sealed = tx.commit().expect("commit");
        assert_eq!(sealed.seq, 3);
        assert_eq!(
            kernel.commit_at_seq(3).expect("read").expect("row").kind,
            "correct"
        );
        id
    };
    let reversal = event_field(&kernel, "superseded_by", &original).expect("reversal");
    assert_ne!(reversal, replacement_id);
    assert_eq!(event_field(&kernel, "recipe_instance_id", &reversal), None);
    assert_eq!(
        event_field(&kernel, "reverses_id", &reversal),
        Some(original.clone())
    );
    assert_eq!(
        event_field(&kernel, "recipe_instance_id", &replacement_id),
        Some(uid("RR01"))
    );
    assert_eq!(
        event_field(&kernel, "counterpart_entity_id", &replacement_id),
        Some(uid("XENT"))
    );
    assert_eq!(current_halves(&kernel, &uid("RR01")), 1);
    assert!(expectation_rows(&kernel, &reversal).is_empty());
    assert_eq!(
        expectation_rows(&kernel, &replacement_id),
        vec![("USD".to_string(), MINOR - 100)]
    );

    let retired = {
        let tmp = Tmp::new("retired");
        let mut kernel = open_at(&tmp.db());
        let books = chart(&mut kernel);
        let original = {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            let id = tx
                .stage_event(recipe_half(&books, "EV20", "PS20", "PS21", MINOR, MINOR))
                .expect("half");
            tx.commit().expect("commit");
            id
        };
        {
            let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
            assert!(tx.correct_event(&original, None).expect("retire").is_none());
            tx.commit().expect("commit");
        }
        let err = {
            let mut tx = kernel.begin_transaction(Some(3)).expect("begin");
            tx.stage_event(recipe_half(&books, "EV24", "PS32", "PS33", MINOR, MINOR))
                .expect_err("retired")
        };
        let err = code_of(err, Code::IdempotencyConflict);
        assert_eq!(err.ids[0], uid("RR01"));
        assert_eq!(current_halves(&kernel, &uid("RR01")), 0);
        err
    };
    assert_eq!(retired.code, Code::IdempotencyConflict);
}

#[test]
fn posting_rejections_and_leaf_success() {
    let tmp = Tmp::new("post");
    let mut kernel = open_at(&tmp.db());
    let books = chart(&mut kernel);

    let reject = |kernel: &mut Kernel, new: NewEvent, code: Code| {
        let err = {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            tx.stage_event(new).expect_err("stage")
        };
        let err = code_of(err, code);
        assert_eq!(kernel.posting_count().expect("count"), 0);
        assert_eq!(kernel.revision().expect("rev"), 1);
        err
    };

    let nonleaf = reject(
        &mut kernel,
        purchase(&books, "PS01", "PS02", &books.food, MINOR, "USD"),
        Code::NonLeafCategory,
    );
    assert_eq!(nonleaf.refs, vec!["INV-003".to_string()]);
    assert_eq!(nonleaf.ids, vec![books.food.clone()]);

    let currency = reject(
        &mut kernel,
        purchase(&books, "PS01", "PS02", &books.groceries, MINOR, "EUR"),
        Code::CurrencyMismatch,
    );
    assert_eq!(currency.class, Class::User);
    assert_eq!(currency.refs, vec!["INV-005".to_string()]);
    assert_eq!(
        currency.ids,
        vec![books.card.clone(), books.card_fin.clone()]
    );
    let text = currency.to_string();
    assert!(
        !text.contains("EUR") && !text.contains("1500"),
        "currency mismatch leaked a payload"
    );

    let mut unbalanced = purchase(&books, "PS01", "PS02", &books.groceries, MINOR, "USD");
    unbalanced.postings[1].amount_minor = MINOR - 100;
    let unbalanced = reject(&mut kernel, unbalanced, Code::Unbalanced);
    assert_eq!(unbalanced.refs, vec!["INV-001".to_string()]);
    assert!(
        !unbalanced.to_string().contains("1500"),
        "unbalanced leaked a magnitude"
    );

    reject(
        &mut kernel,
        event(
            "EV01",
            vec![posting("PS01", &books.groceries, Side::Debit, MINOR, "USD")],
        ),
        Code::InsufficientPostings,
    );
    reject(
        &mut kernel,
        purchase(&books, "PS01", "PS02", &books.groceries, 0, "USD"),
        Code::InvalidMoney,
    );
    let mut unknown = purchase(&books, "PS01", "PS02", &books.groceries, MINOR, "USD");
    unknown.recipe_id = "missing".to_string();
    reject(&mut kernel, unknown, Code::UnknownRecipe);
    let mut dated = purchase(&books, "PS01", "PS02", &books.groceries, MINOR, "USD");
    dated.effective_date = "2026-13-40".to_string();
    reject(&mut kernel, dated, Code::InvalidDate);
    let mut missing = purchase(&books, "PS01", "PS02", &books.groceries, MINOR, "USD");
    missing.postings[0].ledger_account_id = uid("ZZZZ");
    reject(&mut kernel, missing, Code::UnknownTarget);

    let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
    tx.stage_event(purchase(
        &books,
        "PS01",
        "PS02",
        &books.groceries,
        MINOR,
        "USD",
    ))
    .expect("leaf");
    let sealed = tx.commit().expect("commit");
    assert_eq!(sealed.seq, 2);
    assert_eq!(
        kernel.commit_at_seq(2).expect("read").expect("row").kind,
        "interpret"
    );
    assert_eq!(kernel.posting_count().expect("count"), 2);

    let child = {
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        tx.create_ledger_account(ledger(
            &uid("TAKE"),
            None,
            AccountType::Expense,
            LedgerKind::Category,
            Some(&books.groceries),
            "Takeout",
        ))
        .expect_err("parent")
    };
    let child = code_of(child, Code::CategoryParentInvalid);
    assert_eq!(child.refs, vec!["INV-003".to_string()]);
    assert!(kernel.ledger_account(&uid("TAKE")).expect("read").is_none());
    assert_eq!(kernel.posting_count().expect("count"), 2);
}

#[test]
fn account_and_party_errors() {
    let tmp = Tmp::new("shape");
    let mut kernel = open_at(&tmp.db());

    let uninit = {
        let mut tx = kernel.begin_transaction(Some(0)).expect("begin");
        tx.create_ledger_account(ledger(
            &uid("CSA1"),
            None,
            AccountType::Asset,
            LedgerKind::Cash,
            None,
            "Cash",
        ))
        .expect_err("uninit")
    };
    let uninit = code_of(uninit, Code::DatabaseUninitialized);
    assert_eq!(uninit.refs, vec!["ADR 0004".to_string()]);
    assert_eq!(kernel.revision().expect("rev"), 0);

    let books = chart(&mut kernel);
    let reject = |kernel: &mut Kernel,
                  run: fn(&mut Transaction<'_>, &Books) -> Result<(), Error>,
                  code: Code| {
        let err = {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            run(&mut tx, &books).expect_err("reject")
        };
        let err = code_of(err, code);
        assert_eq!(kernel.revision().expect("rev"), 1);
        err
    };

    let same_id = reject(
        &mut kernel,
        |tx, _| {
            tx.create_entity(NewEntity {
                id: Some(uid("ENT1")),
                party_id: Some(uid("ENT1")),
                name: "Household".to_string(),
            })
            .map(|_| ())
        },
        Code::InvalidShape,
    );
    assert_eq!(same_id.token, Some("unique_other"));
    let second = reject(
        &mut kernel,
        |tx, _| {
            tx.create_entity(NewEntity {
                id: Some(uid("XENT")),
                party_id: Some(uid("EPT1")),
                name: "Other".to_string(),
            })
            .map(|_| ())
        },
        Code::SecondEntity,
    );
    assert_eq!(second.refs, vec!["ADR 0004".to_string()]);
    assert!(kernel.entity().expect("read").expect("entity").id == books.entity);

    let duplicate = reject(
        &mut kernel,
        |tx, books| {
            tx.create_party(party(&books.employer, "Acme Corp", PartyLabel::Employer))
                .map(|_| ())
        },
        Code::DuplicateId,
    );
    assert_eq!(duplicate.ids, vec![books.employer.clone()]);
    let invalid = reject(
        &mut kernel,
        |tx, _| {
            tx.create_party(party("not-a-ulid", "Example Grocer", PartyLabel::Vendor))
                .map(|_| ())
        },
        Code::InvalidId,
    );
    assert!(invalid.ids.is_empty());
    assert!(invalid.token.is_none());
    let empty = {
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        tx.create_party(party(&uid("VND1"), "", PartyLabel::Vendor))
            .expect_err("name")
    };
    let empty = code_of(empty, Code::InvalidShape);
    assert!(empty.token.is_none());

    reject(
        &mut kernel,
        |tx, _| {
            tx.create_party(NewParty {
                id: Some(uid("VND1")),
                name: "Example Grocer".to_string(),
                label: PartyLabel::Vendor,
                parent_id: Some(uid("VND1")),
            })
            .map(|_| ())
        },
        Code::UnknownTarget,
    );
    reject(
        &mut kernel,
        |tx, _| {
            tx.create_party(NewParty {
                id: Some(uid("VND1")),
                name: "Example Grocer".to_string(),
                label: PartyLabel::Vendor,
                parent_id: Some(uid("ZZZZ")),
            })
            .map(|_| ())
        },
        Code::UnknownTarget,
    );
    let alias = reject(
        &mut kernel,
        |tx, books| {
            tx.add_party_alias(&books.employer, "Example Grocer")
                .map(|_| ())
        },
        Code::InvalidShape,
    );
    assert_eq!(alias.token, Some("unique_other"));
    reject(
        &mut kernel,
        |tx, _| {
            tx.add_party_alias(&uid("ZZZZ"), "Example Grocer")
                .map(|_| ())
        },
        Code::UnknownTarget,
    );
    let missing_name = code_of(
        {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            tx.rename(ReferenceKind::Party, &uid("ZZZZ"), "Replacement")
                .expect_err("rename")
        },
        Code::RenameTargetMissing,
    );
    assert_eq!(missing_name.ids, vec![uid("ZZZZ")]);

    for currency in ["usd", "US", "USDD"] {
        let err = {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            tx.create_financial_account(financial(
                &uid("CSH1"),
                "Checking",
                FinancialKind::Checking,
                currency,
                None,
            ))
            .expect_err("currency")
        };
        code_of(err, Code::InvalidCurrency);
    }
    let last4 = {
        let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
        tx.create_financial_account(financial(
            &uid("CSH1"),
            "Checking",
            FinancialKind::Checking,
            "USD",
            Some("12ab"),
        ))
        .expect_err("last4")
    };
    assert_eq!(code_of(last4, Code::InvalidShape).token, Some("last4"));

    let parent = code_of(
        {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            tx.create_ledger_account(ledger(
                &uid("TAKE"),
                None,
                AccountType::Expense,
                LedgerKind::Category,
                Some(&uid("ZZZZ")),
                "Takeout",
            ))
            .expect_err("parent")
        },
        Code::CategoryParentInvalid,
    );
    assert_eq!(parent.refs, vec!["INV-003".to_string()]);

    let typed = code_of(
        {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            tx.create_ledger_account(ledger(
                &uid("NC01"),
                None,
                AccountType::Income,
                LedgerKind::Category,
                None,
                "Wages",
            ))
            .expect("income");
            tx.create_ledger_account(ledger(
                &uid("TAKE"),
                None,
                AccountType::Expense,
                LedgerKind::Category,
                Some(&uid("NC01")),
                "Takeout",
            ))
            .expect_err("type")
        },
        Code::CategoryParentInvalid,
    );
    assert_eq!(
        typed.refs,
        vec!["INV-003".to_string(), "ADR 0008".to_string()]
    );

    let mismatch = code_of(
        {
            let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
            tx.create_financial_account(financial(
                &uid("CSH1"),
                "Checking",
                FinancialKind::Checking,
                "USD",
                None,
            ))
            .expect("checking");
            tx.create_ledger_account(ledger(
                &uid("CRD1"),
                Some(&uid("CSH1")),
                AccountType::Liability,
                LedgerKind::Card,
                None,
                "Card",
            ))
            .expect_err("kind")
        },
        Code::LedgerKindMismatch,
    );
    assert_eq!(mismatch.refs, vec!["ADR 0008".to_string()]);
    assert!(kernel.ledger_account(&uid("CSA1")).expect("read").is_none());
}

#[test]
fn legal_kind_pairs_commit_and_illegal_pairs_do_not() {
    let tmp = Tmp::new("kinds");
    let mut kernel = open_at(&tmp.db());
    let _books = chart(&mut kernel);
    let mut tx = kernel.begin_transaction(Some(1)).expect("begin");
    tx.create_financial_account(NewFinancialAccount {
        id: Some(uid("CSH1")),
        name: "Checking".to_string(),
        kind: FinancialKind::Checking,
        currency: "USD".to_string(),
        tracking_mode: TrackingMode::Transparent,
        institution: Some(String::new()),
        last4: None,
    })
    .expect("checking");
    tx.create_financial_account(financial(
        &uid("N011"),
        "Loan",
        FinancialKind::Loan,
        "USD",
        None,
    ))
    .expect("loan");
    tx.create_financial_account(financial(
        &uid("BRK1"),
        "Brokerage",
        FinancialKind::Brokerage,
        "USD",
        None,
    ))
    .expect("brokerage");
    tx.create_financial_account(financial(
        &uid("REST"),
        "House",
        FinancialKind::RealEstate,
        "USD",
        None,
    ))
    .expect("estate");
    let pairs = [
        (
            uid("CSA1"),
            Some(uid("CSH1")),
            AccountType::Asset,
            LedgerKind::Cash,
            "Cash",
        ),
        (
            uid("ND01"),
            Some(uid("N011")),
            AccountType::Liability,
            LedgerKind::Loan,
            "Loan",
        ),
        (
            uid("NV02"),
            Some(uid("BRK1")),
            AccountType::Asset,
            LedgerKind::Investment,
            "Brokerage",
        ),
        (
            uid("FX01"),
            Some(uid("REST")),
            AccountType::Asset,
            LedgerKind::FixedAsset,
            "House",
        ),
        (
            uid("RCV1"),
            None,
            AccountType::Asset,
            LedgerKind::Receivable,
            "Receivable",
        ),
        (
            uid("PAY1"),
            None,
            AccountType::Liability,
            LedgerKind::Payable,
            "Payable",
        ),
        (
            uid("C001"),
            None,
            AccountType::Asset,
            LedgerKind::Clearing,
            "Clearing",
        ),
        (
            uid("EQ01"),
            None,
            AccountType::Equity,
            LedgerKind::OpeningEquity,
            "Opening",
        ),
        (
            uid("NC01"),
            None,
            AccountType::Income,
            LedgerKind::Category,
            "Wages",
        ),
    ];
    for (id, financial_id, account_type, kind, name) in pairs {
        tx.create_ledger_account(ledger(
            &id,
            financial_id.as_deref(),
            account_type,
            kind,
            None,
            name,
        ))
        .expect(name);
    }
    assert_eq!(tx.commit().expect("commit").seq, 2);
    let institution: Option<String> = kernel
        .connection()
        .query_row(
            "SELECT institution FROM financial_accounts WHERE id = ?1",
            [uid("CSH1")],
            |row| row.get(0),
        )
        .expect("institution");
    assert!(institution.is_none(), "empty institution was stored");
    assert_eq!(
        kernel
            .ledger_account(&uid("EQ01"))
            .expect("read")
            .expect("equity")
            .kind,
        "opening_equity"
    );

    let illegal = |kernel: &mut Kernel, new: NewLedgerAccount| {
        let mut tx = kernel.begin_transaction(Some(2)).expect("begin");
        tx.create_ledger_account(new).expect_err("illegal")
    };
    assert_eq!(
        code_of(
            illegal(
                &mut kernel,
                ledger(
                    &uid("TAKE"),
                    Some(&uid("CSH1")),
                    AccountType::Expense,
                    LedgerKind::Category,
                    None,
                    "Food",
                ),
            ),
            Code::InvalidShape,
        )
        .token,
        Some("ledger_kind")
    );
    assert_eq!(
        code_of(
            illegal(
                &mut kernel,
                ledger(
                    &uid("TAKE"),
                    None,
                    AccountType::Liability,
                    LedgerKind::Cash,
                    None,
                    "Cash"
                ),
            ),
            Code::InvalidShape,
        )
        .token,
        Some("ledger_kind")
    );
    assert_eq!(
        code_of(
            illegal(
                &mut kernel,
                ledger(
                    &uid("TAKE"),
                    None,
                    AccountType::Expense,
                    LedgerKind::Receivable,
                    None,
                    "Due",
                ),
            ),
            Code::InvalidShape,
        )
        .token,
        Some("ledger_kind")
    );
    assert_eq!(
        code_of(
            illegal(
                &mut kernel,
                ledger(
                    &uid("TAKE"),
                    None,
                    AccountType::Expense,
                    LedgerKind::Clearing,
                    None,
                    "Clearing",
                ),
            ),
            Code::InvalidShape,
        )
        .token,
        Some("ledger_kind")
    );
    assert_eq!(kernel.revision().expect("rev"), 2);
}

#[test]
fn parsers_reject_contra_and_unknown_labels() {
    assert_eq!(
        AccountType::parse("contra").expect_err("type").token,
        Some("account_type")
    );
    assert_eq!(
        LedgerKind::parse("contra").expect_err("kind").token,
        Some("ledger_kind")
    );
    assert_eq!(
        FinancialKind::parse("paypal").expect_err("fin").token,
        Some("financial_kind")
    );
    assert_eq!(
        TrackingMode::parse("hidden").expect_err("track").token,
        Some("tracking_mode")
    );
    assert_eq!(
        PartyLabel::parse("merchant").expect_err("label").token,
        Some("party_label")
    );
    assert_eq!(
        Side::parse("both").expect_err("side").token,
        Some("posting_side")
    );
    assert_eq!(Code::CurrencyMismatch.class(), Class::User);
    assert!(!persists_freeze(Code::CurrencyMismatch));
    assert!(!persists_freeze(Code::ForeignKeysDisabled));
    assert!(persists_freeze(Code::UnsealedRows));
    let shown = Error::new(Code::StaleRevision, &["AT-032"], vec![]);
    assert_eq!(
        shown.to_string(),
        "code=StaleRevision class=user refs=AT-032"
    );
    match (Subject::Observation {
        feed_id: "feed".to_string(),
        external_id: "ext".to_string(),
        commit_id: uid("CMT1"),
        seq: 2,
    }) {
        Subject::Observation { seq, .. } => assert_eq!(seq, 2),
        Subject::RecipeInstance { .. } => panic!("observation subject"),
    }
}

#[test]
fn storage_message_map() {
    let recipe = map_message(
        "UNIQUE constraint failed: accounting_events.recipe_instance_id",
        SqlSite::Staging,
    );
    assert_eq!(recipe.code, Code::IdempotencyConflict);
    assert_eq!(recipe.refs, vec!["ADR 0004".to_string()]);
    let itself = map_message(
        "ADR 0004: counterpart_entity_id must be another entity",
        SqlSite::Staging,
    );
    assert_eq!(itself.code, Code::InvalidShape);
    assert_eq!(itself.token, Some("counterpart_self"));
    let fields = map_message(
        "CHECK constraint failed: (recipe_instance_id IS NULL) = (counterpart_entity_id IS NULL)",
        SqlSite::Staging,
    );
    assert_eq!(fields.token, Some("recipe_fields"));
    let duplicate = map_message(
        "UNIQUE constraint failed: event_counterpart_expectations.event_id, event_counterpart_expectations.currency",
        SqlSite::Staging,
    );
    assert_eq!(duplicate.token, Some("counterpart_expectations"));
    let missing = map_message(
        "ADR 0004: a recipe half needs at least one counterpart expectation",
        SqlSite::Seal,
    );
    assert_eq!(missing.code, Code::ValidatorMissed);
    let amount = map_message(
        "CHECK constraint failed: expected_amount_minor > 0",
        SqlSite::Staging,
    );
    assert_eq!(amount.code, Code::InvalidMoney);

    let revision = map_message(
        "CHECK constraint failed: seq = expected_revision + 1",
        SqlSite::Seal,
    );
    assert_eq!(revision.code, Code::RevisionMismatch);
    assert_eq!(revision.class, Class::User);

    let seal = map_message(
        "AT-032: stale revision (expected_revision is not the current revision)",
        SqlSite::Seal,
    );
    assert_eq!(seal.code, Code::ValidatorMissed);
    assert_eq!(seal.class, Class::Bugcheck);
    assert_eq!(seal.refs, vec!["StaleRevision".to_string()]);
    let staging = map_message(
        "AT-032: stale revision (expected_revision is not the current revision)",
        SqlSite::Staging,
    );
    assert_eq!(staging.code, Code::StaleRevision);
    assert_eq!(staging.class, Class::User);

    let currency = map_message(
        "INV-005: posting currency must match the financial account currency",
        SqlSite::Staging,
    );
    assert_eq!(currency.code, Code::CurrencyMismatch);
    assert_eq!(currency.class, Class::User);
    assert_eq!(currency.refs, vec!["INV-005".to_string()]);

    assert_eq!(
        map_message("UNIQUE constraint failed: parties.id", SqlSite::Staging).code,
        Code::DuplicateId
    );
    assert_eq!(
        map_message(
            "UNIQUE constraint failed: entity.singleton",
            SqlSite::Staging
        )
        .code,
        Code::SecondEntity
    );
    assert_eq!(
        map_message(
            "UNIQUE constraint failed: entity.party_id",
            SqlSite::Staging
        )
        .token,
        Some("unique_other")
    );
    assert_eq!(
        map_message(
            "ADR 0008: ledger accounts are immutable; rename via reference_renames",
            SqlSite::Staging,
        )
        .code,
        Code::AppendOnlyViolation
    );
    let partial = map_message(
        "FOREIGN KEY constraint failed",
        SqlSite::Finalize {
            commits_inserted: false,
        },
    );
    assert_eq!(partial.code, Code::PartialCommit);
    assert_eq!(partial.class, Class::Bugcheck);
    let missed = map_message(
        "FOREIGN KEY constraint failed",
        SqlSite::Finalize {
            commits_inserted: true,
        },
    );
    assert_eq!(missed.code, Code::ValidatorMissed);
    assert_eq!(missed.refs, vec!["UnknownTarget".to_string()]);
    assert_eq!(
        map_message(
            "INV-003/ADR 0008: income/expense postings must use a leaf category account",
            SqlSite::Staging
        )
        .code,
        Code::NonLeafCategory
    );
    assert_eq!(
        map_message("no such rule", SqlSite::Staging).code,
        Code::UnmappedStorageError
    );
}

#[test]
fn bad_clock_writes_nothing() {
    let tmp = Tmp::new("clock");
    struct BadClock;
    impl Clock for BadClock {
        fn now_utc(&mut self) -> String {
            "not-a-timestamp".to_string()
        }
    }
    let mut kernel = Kernel::open(
        tmp.db(),
        Box::new(BadClock),
        Box::new(SequenceIds::new()),
        "u17",
    )
    .expect("open");
    let err = code_of(
        kernel.begin_transaction(Some(0)).expect_err("clock"),
        Code::InvalidDate,
    );
    assert!(err.ids.is_empty());
    assert_eq!(kernel.revision().expect("rev"), 0);
    assert_eq!(kernel.commit_count().expect("count"), 0);
}
