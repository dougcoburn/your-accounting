//! Contract for the committed October 2025 synthetic Monarch fixture.

use std::collections::{BTreeMap, BTreeSet};

use monarch_synth::{
    default_coverage_path, default_csv_path, execute, generate, parse_amount, parse_csv,
    render_coverage, render_csv, Row, COLUMNS, DEFAULT_SEED,
};

const TRANSFER: &[&str] = &[
    "Credit Card Payment",
    "Transfer",
    "Loan Payment",
    "Savings Transfer",
    "Investment Transfer",
    "HSA Contribution",
];

const REQUIRED_SCENARIOS: &[&str] = &[
    "Card buys",
    "Credit-card payment (both sides)",
    "Paycheck",
    "HSA contribution (matching side missing)",
    "Dragonfly e-tagged buy",
    "Numetric partial reimbursement",
    "Refund",
    "Card perk or statement credit",
    "Duplicate row",
    "In-flight item",
];

#[test]
fn same_seed_is_byte_for_byte_deterministic() {
    let first = render_csv(&generate(DEFAULT_SEED));
    let second = render_csv(&generate(DEFAULT_SEED));
    assert_eq!(first, second);
    let other = render_csv(&generate(1));
    assert_ne!(first, other);
    assert_eq!(other, render_csv(&generate(1)));
    assert_eq!(
        render_coverage(&generate(DEFAULT_SEED)),
        render_coverage(&generate(1)),
        "scenario coverage ids are seed-independent"
    );
}

#[test]
fn committed_fixture_matches_the_generator_and_the_contract() {
    let csv = std::fs::read_to_string(default_csv_path()).expect("committed csv");
    let markdown = std::fs::read_to_string(default_coverage_path()).expect("committed coverage");
    let fixture = generate(DEFAULT_SEED);
    assert_eq!(csv, render_csv(&fixture), "csv drifted from the generator");
    assert_eq!(
        markdown,
        render_coverage(&fixture),
        "coverage table drifted from the generator"
    );
    let rows = parse_csv(&csv).expect("parse committed csv");
    assert_eq!(rows, fixture.rows);
    assert_contract(&rows, &csv);
    assert_scenarios(&rows, &coverage_table(&markdown));
    assert_no_long_digit_run("coverage", &markdown);
    assert!(
        markdown.starts_with("# synthetic"),
        "coverage file should carry the synthetic marker"
    );
}

#[test]
fn other_seeds_keep_the_same_contract() {
    for seed in [1_u64, u64::MAX] {
        let fixture = generate(seed);
        let csv = render_csv(&fixture);
        let rows = parse_csv(&csv).expect("parse");
        assert_eq!(rows, fixture.rows);
        assert_contract(&rows, &csv);
        assert_scenarios(&rows, &coverage_table(&render_coverage(&fixture)));
    }
}

#[test]
fn fixture_lives_under_fixtures() {
    let csv = default_csv_path().canonicalize().expect("csv exists");
    let coverage = default_coverage_path()
        .canonicalize()
        .expect("coverage exists");
    assert_eq!(csv.parent(), coverage.parent());
    let parts: Vec<String> = csv
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert!(parts.iter().any(|part| part == "fixtures"), "{csv:?}");
    assert!(parts.iter().any(|part| part == "monarch"), "{csv:?}");
    assert_eq!(csv.extension().and_then(|ext| ext.to_str()), Some("csv"));
}

#[test]
fn cli_check_accepts_the_committed_fixture() {
    assert_eq!(execute(["--check"]).unwrap(), 0);
}

#[test]
fn cli_check_rejects_a_different_seed() {
    assert_eq!(execute(["--check", "--seed", "1"]).unwrap(), 1);
}

#[test]
fn cli_seed_write_is_deterministic() {
    let dir = std::env::temp_dir().join(format!("monarch-synth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("out.csv");
    let coverage = dir.join("README.md");
    let args = [
        "--seed",
        "11",
        "--out",
        csv.to_str().unwrap(),
        "--coverage",
        coverage.to_str().unwrap(),
    ];
    assert_eq!(execute(args).unwrap(), 0);
    let first = std::fs::read(&csv).unwrap();
    assert_eq!(execute(args).unwrap(), 0);
    let second = std::fs::read(&csv).unwrap();
    assert_eq!(first, second);
    assert!(first.starts_with(b"# synthetic\n"));
    let markdown = std::fs::read_to_string(&coverage).unwrap();
    assert!(markdown.starts_with("# synthetic"));
    std::fs::remove_dir_all(&dir).ok();
}

fn assert_contract(rows: &[Row], raw: &str) {
    let mut lines = raw.lines();
    assert_eq!(lines.next(), Some("# synthetic"));
    assert_eq!(
        lines.next(),
        Some(
            "Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id"
        )
    );
    assert_eq!(
        COLUMNS.join(","),
        "Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id"
    );
    let header: Vec<&str> = raw.lines().nth(1).unwrap().split(',').collect();
    assert_eq!(header.len(), 14, "expected 14 columns");
    assert_eq!(header.iter().filter(|column| **column == "Date").count(), 1);
    assert!(header
        .iter()
        .all(|column| !column.to_ascii_lowercase().contains("posted")));
    assert!(
        raw.lines().count() > rows.len() + 2,
        "a quoted field should span a physical line"
    );
    assert!(!raw.contains('\r'));
    assert_no_long_digit_run("csv", raw);

    assert!(
        (190..=210).contains(&rows.len()),
        "row count {}",
        rows.len()
    );

    let mut ids = BTreeSet::new();
    let mut accounts = BTreeSet::new();
    let mut categories = BTreeSet::new();
    let mut tags = BTreeSet::new();
    let mut owners = BTreeSet::new();
    let mut reviewers = BTreeSet::new();
    let mut negative = 0;
    let mut positive = 0;
    let mut transfer = 0;
    let mut untagged = 0;
    let mut noted = 0;
    let mut multiline = 0;

    for row in rows {
        assert!(!row.id.is_empty(), "blank id");
        assert!(ids.insert(row.id.clone()), "duplicate id {}", row.id);
        assert!(is_october_2025(&row.date), "{} date {}", row.id, row.date);
        assert!(
            account_has_only_a_last4(&row.account),
            "{} account {}",
            row.id,
            row.account
        );
        accounts.insert(row.account.clone());
        assert!(!row.category.is_empty());
        categories.insert(row.category.clone());
        owners.insert(row.owner.clone());
        reviewers.insert(row.reviewed_by.clone());
        assert_eq!(row.reviewed, "true", "{}", row.id);
        assert_eq!(row.pending, "false", "{}", row.id);
        assert!(!row.reviewed_at.is_empty(), "{}", row.id);
        assert!(!row.reviewed_by.is_empty(), "{}", row.id);
        let cents = parse_amount(&row.amount).expect(&row.id);
        assert_ne!(cents, 0, "{}", row.id);
        if cents < 0 {
            negative += 1;
        } else {
            positive += 1;
        }
        if TRANSFER.contains(&row.category.as_str()) {
            transfer += 1;
        }
        if row.tags.is_empty() {
            untagged += 1;
        } else {
            assert!(is_e_tag(&row.tags), "{} tag {}", row.id, row.tags);
            tags.insert(row.tags.clone());
        }
        if !row.notes.is_empty() {
            noted += 1;
        }
        if row.original_statement.contains('\n') || row.notes.contains('\n') {
            multiline += 1;
        }
        for field in [
            row.date.as_str(),
            row.merchant.as_str(),
            row.category.as_str(),
            row.account.as_str(),
            row.original_statement.as_str(),
            row.notes.as_str(),
            row.amount.as_str(),
            row.tags.as_str(),
            row.owner.as_str(),
            row.reviewed.as_str(),
            row.reviewed_at.as_str(),
            row.reviewed_by.as_str(),
            row.pending.as_str(),
            row.id.as_str(),
        ] {
            assert_no_long_digit_run(&row.id, field);
        }
    }

    assert_eq!(negative + positive, rows.len());
    assert!(
        (75..=85).contains(&percent(negative, rows.len())),
        "negative share {negative}/{}",
        rows.len()
    );
    assert!(
        (15..=25).contains(&percent(positive, rows.len())),
        "positive share {positive}/{}",
        rows.len()
    );
    assert_eq!(accounts.len(), 12, "accounts: {accounts:?}");
    assert!(
        (40..=48).contains(&categories.len()),
        "categories {}",
        categories.len()
    );
    assert!(
        (12..=18).contains(&percent(transfer, rows.len())),
        "transfer share {transfer}/{}",
        rows.len()
    );
    assert_eq!(tags.len(), 7, "tags: {tags:?}");
    assert!((1..=3).contains(&untagged), "untagged rows {untagged}");
    assert!((4..=12).contains(&noted), "rows with notes {noted}");
    assert!(multiline >= 1, "expected a multiline quoted field");
    assert_eq!(owners.len(), 1, "owners: {owners:?}");
    assert_eq!(
        owners.iter().next().map(String::as_str),
        Some("Casey Sample")
    );
    assert_eq!(reviewers.len(), 1, "reviewers: {reviewers:?}");
    assert!(card_buys(rows) >= 40, "card buys {}", card_buys(rows));
}

fn assert_scenarios(rows: &[Row], table: &BTreeMap<String, Vec<String>>) {
    assert_eq!(
        table.len(),
        REQUIRED_SCENARIOS.len(),
        "scenarios: {:?}",
        table.keys().collect::<Vec<_>>()
    );
    for name in REQUIRED_SCENARIOS {
        assert!(table.contains_key(*name), "missing scenario {name}");
    }

    let card_buys = &table["Card buys"];
    assert!(card_buys.len() >= 3);
    for id in card_buys {
        let row = by_id(rows, id);
        assert!(cents(row) < 0, "{id}");
        assert!(is_credit_card(&row.account), "{id} {}", row.account);
        assert!(!TRANSFER.contains(&row.category.as_str()), "{id}");
    }

    let payment = &table["Credit-card payment (both sides)"];
    assert_eq!(payment.len(), 2);
    let left = by_id(rows, &payment[0]);
    let right = by_id(rows, &payment[1]);
    assert_eq!(left.category, "Credit Card Payment");
    assert_eq!(right.category, "Credit Card Payment");
    assert_eq!(left.date, right.date);
    assert_eq!(cents(left), -cents(right));
    let (outflow, inflow) = if cents(left) < 0 {
        (left, right)
    } else {
        (right, left)
    };
    assert!(outflow.account.contains("Checking"), "{}", outflow.account);
    assert!(is_credit_card(&inflow.account), "{}", inflow.account);

    for id in &table["Paycheck"] {
        let row = by_id(rows, id);
        assert_eq!(row.category, "Paychecks");
        assert!(cents(row) > 0, "{id}");
        assert!(row.merchant.contains("Numetric"), "{id}");
        assert!(row.merchant.contains("fictional"), "{id}");
        assert_eq!(row.tags, "e-numetric");
    }
    assert!(!table["Paycheck"].is_empty());

    let hsa_ids = &table["HSA contribution (matching side missing)"];
    assert_eq!(hsa_ids.len(), 1);
    let hsa = by_id(rows, &hsa_ids[0]);
    assert_eq!(hsa.category, "HSA Contribution");
    assert!(cents(hsa) < 0);
    assert!(hsa.account.contains("Checking"), "{}", hsa.account);
    assert!(hsa.notes.to_ascii_lowercase().contains("missing"));
    assert!(rows
        .iter()
        .all(|row| !row.account.to_ascii_lowercase().contains("hsa")));
    assert!(rows.iter().all(|row| cents(row) != -cents(hsa)));
    assert_eq!(
        rows.iter()
            .filter(|row| row.category == "HSA Contribution")
            .count(),
        1
    );

    let dragonfly = &table["Dragonfly e-tagged buy"];
    assert!(!dragonfly.is_empty());
    for id in dragonfly {
        let row = by_id(rows, id);
        assert_eq!(row.tags, "e-dragonfly");
        assert!(cents(row) < 0, "{id}");
        assert!(!TRANSFER.contains(&row.category.as_str()), "{id}");
        assert!(is_credit_card(&row.account), "{id} {}", row.account);
        assert!(
            row.notes.contains("Dragonfly Homes"),
            "{id} notes {}",
            row.notes
        );
    }

    let numetric = &table["Numetric partial reimbursement"];
    assert_eq!(numetric.len(), 2);
    let pair: Vec<&Row> = numetric.iter().map(|id| by_id(rows, id)).collect();
    assert!(pair.iter().all(|row| row.tags == "e-numetric"));
    let expense = pair
        .iter()
        .copied()
        .find(|row| cents(row) < 0)
        .expect("numetric expense");
    let reimbursement = pair
        .iter()
        .copied()
        .find(|row| cents(row) > 0)
        .expect("numetric reimbursement");
    assert_eq!(reimbursement.category, "Reimbursements");
    assert!(!TRANSFER.contains(&expense.category.as_str()));
    assert!(cents(reimbursement) < -cents(expense));
    assert!(reimbursement.notes.to_ascii_lowercase().contains("partial"));
    assert!(reimbursement.merchant.contains("Numetric"));
    assert!(expense.merchant.contains("fictional"));

    for id in &table["Refund"] {
        let row = by_id(rows, id);
        assert!(cents(row) > 0, "{id}");
        assert_eq!(row.category, "Groceries");
        assert!(is_credit_card(&row.account), "{}", row.account);
        assert!(row.original_statement.contains("RETURN"));
        assert!(rows.iter().any(|other| {
            other.id != row.id
                && other.merchant == row.merchant
                && other.category == row.category
                && cents(other) < 0
        }));
    }
    assert!(!table["Refund"].is_empty());

    for id in &table["Card perk or statement credit"] {
        let row = by_id(rows, id);
        assert!(cents(row) > 0, "{id}");
        assert_eq!(row.category, "Subscriptions");
        assert!(is_credit_card(&row.account), "{}", row.account);
        assert!(row.original_statement.contains("STATEMENT CREDIT"));
        assert!(!TRANSFER.contains(&row.category.as_str()));
    }
    assert!(!table["Card perk or statement credit"].is_empty());

    let duplicates = &table["Duplicate row"];
    assert_eq!(duplicates.len(), 2);
    let first = by_id(rows, &duplicates[0]);
    let second = by_id(rows, &duplicates[1]);
    assert_ne!(first.id, second.id);
    assert_eq!(content_key(first), content_key(second));
    assert!(cents(first) < 0);
    assert!(!TRANSFER.contains(&first.category.as_str()));
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        groups
            .entry(content_key(row))
            .or_default()
            .push(row.id.clone());
    }
    let dup_groups: Vec<_> = groups.values().filter(|ids| ids.len() > 1).collect();
    assert_eq!(dup_groups.len(), 1, "duplicate groups: {dup_groups:?}");
    assert_eq!(dup_groups[0].len(), 2);

    let inflight_ids = &table["In-flight item"];
    assert_eq!(inflight_ids.len(), 1);
    let inflight = by_id(rows, &inflight_ids[0]);
    assert!(inflight.date.as_str() >= "2025-10-28", "{}", inflight.date);
    assert!(TRANSFER.contains(&inflight.category.as_str()));
    assert!(cents(inflight) < 0);
    assert!(inflight.notes.contains("November"), "{}", inflight.notes);
    assert!(rows.iter().all(|row| cents(row) != -cents(inflight)));
}

fn coverage_table(markdown: &str) -> BTreeMap<String, Vec<String>> {
    let mut table = BTreeMap::new();
    for line in markdown.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 2 || cells[0] == "Scenario" || cells[0].starts_with('-') {
            continue;
        }
        let ids = cells[1]
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect();
        table.insert(cells[0].to_string(), ids);
    }
    table
}

fn by_id<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("missing id {id}"))
}

fn cents(row: &Row) -> i64 {
    parse_amount(&row.amount).unwrap_or_else(|err| panic!("{}: {err}", row.id))
}

fn content_key(row: &Row) -> String {
    [
        row.date.as_str(),
        row.merchant.as_str(),
        row.category.as_str(),
        row.account.as_str(),
        row.original_statement.as_str(),
        row.notes.as_str(),
        row.amount.as_str(),
        row.tags.as_str(),
        row.owner.as_str(),
        row.reviewed.as_str(),
        row.reviewed_at.as_str(),
        row.reviewed_by.as_str(),
        row.pending.as_str(),
    ]
    .join("\u{1f}")
}

fn card_buys(rows: &[Row]) -> usize {
    rows.iter()
        .filter(|row| {
            cents(row) < 0
                && is_credit_card(&row.account)
                && !TRANSFER.contains(&row.category.as_str())
        })
        .count()
}

fn is_credit_card(account: &str) -> bool {
    matches!(
        account,
        "Cinder Visa ...5582"
            | "Cinder Mastercard ...5590"
            | "Fable Rewards ...6603"
            | "Nimbus Store Card ...7714"
    )
}

fn is_october_2025(date: &str) -> bool {
    let Some(day) = date.strip_prefix("2025-10-") else {
        return false;
    };
    matches!(day.parse::<u32>(), Ok(day) if (1..=31).contains(&day)) && day.len() == 2
}

fn is_e_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix("e-") else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn account_has_only_a_last4(account: &str) -> bool {
    if !account.contains("...") {
        return false;
    }
    let mut runs = 0;
    let mut run = 0;
    for byte in account.bytes() {
        if byte.is_ascii_digit() {
            run += 1;
            if run > 4 {
                return false;
            }
        } else if run > 0 {
            if run != 4 {
                return false;
            }
            runs += 1;
            run = 0;
        }
    }
    if run > 0 {
        if run != 4 {
            return false;
        }
        runs += 1;
    }
    runs == 1
}

fn percent(part: usize, whole: usize) -> usize {
    part * 100 / whole
}

fn assert_no_long_digit_run(label: &str, text: &str) {
    let bytes = text.as_bytes();
    let mut run = 0;
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if byte.is_ascii_digit() {
            if run == 0 {
                start = index;
            }
            run += 1;
            if run > 4 {
                let from = start.saturating_sub(20);
                let to = (index + 20).min(bytes.len());
                let snippet = String::from_utf8_lossy(&bytes[from..to]);
                panic!("{label} contains a number longer than 4 digits near `{snippet}`");
            }
        } else {
            run = 0;
        }
    }
}
