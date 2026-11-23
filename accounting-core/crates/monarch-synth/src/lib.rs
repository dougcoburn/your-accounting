//! Seeded, deterministic synthetic Monarch Money export for October 2025.
//!
//! Every merchant, note, and id is invented. Account names carry at most a
//! last-4. The same seed always renders the same CSV bytes. Scenario rows keep
//! their ids for every seed; the seed only changes filler merchants, amounts,
//! and days.

mod cli;

use std::collections::BTreeSet;
use std::path::PathBuf;

pub use cli::execute;

/// Digits of 2025-10-15. The underscores follow the date, not thousands.
#[allow(clippy::inconsistent_digit_grouping)]
pub const DEFAULT_SEED: u64 = 2025_10_15;
pub const MARKER: &str = "# synthetic";
pub const COLUMNS: [&str; 14] = [
    "Date",
    "Merchant",
    "Category",
    "Account",
    "Original Statement",
    "Notes",
    "Amount",
    "Tags",
    "Owner",
    "Reviewed",
    "Reviewed At",
    "Reviewed By",
    "Pending",
    "Id",
];

const OWNER: &str = "Casey Sample";
const REVIEWED_BY: &str = "Rowan Checker";

const PEBBLE_CHECKING: &str = "Pebble Checking ...2210";
const PEBBLE_SAVINGS: &str = "Pebble Savings ...2218";
const HARBOR_JOINT: &str = "Harbor Joint Checking ...3340";
const LUMEN_CASH: &str = "Lumen Cash ...4471";
const CINDER_VISA: &str = "Cinder Visa ...5582";
const CINDER_MC: &str = "Cinder Mastercard ...5590";
const FABLE: &str = "Fable Rewards ...6603";
const NIMBUS: &str = "Nimbus Store Card ...7714";
const DRAGONFLY: &str = "Dragonfly Operating ...9936";
const OAK: &str = "Oak Mortgage ...1048";
const SABLE: &str = "Sable Auto Loan ...2059";
const BRAMBLE: &str = "Bramble Brokerage ...3160";

const ACCOUNTS: [&str; 12] = [
    PEBBLE_CHECKING,
    PEBBLE_SAVINGS,
    HARBOR_JOINT,
    LUMEN_CASH,
    CINDER_VISA,
    CINDER_MC,
    FABLE,
    NIMBUS,
    DRAGONFLY,
    OAK,
    SABLE,
    BRAMBLE,
];
const CHECKING: [&str; 3] = [PEBBLE_CHECKING, HARBOR_JOINT, LUMEN_CASH];
const CARDS: [&str; 4] = [CINDER_VISA, CINDER_MC, FABLE, NIMBUS];

const TRANSFER: [&str; 6] = [
    "Credit Card Payment",
    "Transfer",
    "Loan Payment",
    "Savings Transfer",
    "Investment Transfer",
    "HSA Contribution",
];
const INCOME: [&str; 7] = [
    "Paychecks",
    "Interest",
    "Dividends",
    "Other Income",
    "Business Income",
    "Gifts Received",
    "Reimbursements",
];
const EXPENSE: [&str; 31] = [
    "Groceries",
    "Restaurants & Bars",
    "Coffee Shops",
    "Gas",
    "Public Transit",
    "Taxi & Rideshare",
    "Auto Maintenance",
    "Medical",
    "Pharmacy",
    "Fitness",
    "Insurance",
    "Utilities",
    "Internet & Cable",
    "Phone",
    "Rent",
    "Mortgage",
    "Home Improvement",
    "Shopping",
    "Clothing",
    "Electronics",
    "Entertainment",
    "Subscriptions",
    "Travel",
    "Pets",
    "Personal Care",
    "Gifts",
    "Charity",
    "Education",
    "Child Care",
    "Taxes",
    "Fees",
];
const _: () = assert!(TRANSFER.len() + INCOME.len() + EXPENSE.len() == 44);

const TAGS: [&str; 7] = [
    "e-household",
    "e-dragonfly",
    "e-numetric",
    "e-personal",
    "e-sinking",
    "e-taxset",
    "e-sample",
];

const ADJECTIVES: [&str; 12] = [
    "Moth", "Fern", "Pebble", "Lantern", "Moss", "Copper", "Aster", "Quill", "Nettle", "Spoon",
    "Cinder", "Bramble",
];
const NOUNS: [&str; 12] = [
    "Market", "Diner", "Cafe", "Supply", "Workshop", "Transit", "Clinic", "Press", "Depot",
    "Pantry", "Books", "Hardware",
];

const GROCER: &str = "Moth & Maple Grocer (fictional)";
const DINER: &str = "Bluefern Diner (fictional)";

const AMT_GROCER: i64 = -6_418;
const AMT_COFFEE: i64 = -475;
const AMT_PAPER: i64 = -2_840;
const AMT_CARD_PAYMENT: i64 = 45_000;
const AMT_PAYCHECK: i64 = 218_042;
const AMT_HSA: i64 = -12_500;
const AMT_DRAGONFLY: i64 = -18_620;
const AMT_WORKSHOP: i64 = -9_640;
const AMT_REIMBURSE: i64 = 4_000;
const AMT_REFUND: i64 = 1_850;
const AMT_PERK: i64 = 1_299;
const AMT_DINER: i64 = -2_315;
const AMT_INFLIGHT: i64 = -27_500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub date: String,
    pub merchant: String,
    pub category: String,
    pub account: String,
    pub original_statement: String,
    pub notes: String,
    pub amount: String,
    pub tags: String,
    pub owner: String,
    pub reviewed: String,
    pub reviewed_at: String,
    pub reviewed_by: String,
    pub pending: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub name: &'static str,
    pub ids: Vec<String>,
    pub evidence: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixture {
    pub rows: Vec<Row>,
    pub scenarios: Vec<Scenario>,
}

pub fn default_csv_path() -> PathBuf {
    fixture_dir().join("october-2025-synthetic.csv")
}

pub fn default_coverage_path() -> PathBuf {
    fixture_dir().join("README.md")
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/monarch")
}

/// Build the October 2025 fixture for `seed`.
///
/// Panics if a seed would break the row, account, category, sign, or scenario
/// quotas. Those quotas do not depend on the seed.
pub fn generate(seed: u64) -> Fixture {
    assert_catalog_disjoint();
    let mut rows = canonical_rows();
    rows.extend(filler_rows(seed));
    rows.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    let scenarios = scenario_catalog();
    check_shape(&rows, &scenarios);
    Fixture { rows, scenarios }
}

pub fn render_csv(fixture: &Fixture) -> String {
    let mut out = String::new();
    out.push_str(MARKER);
    out.push('\n');
    out.push_str(&COLUMNS.join(","));
    out.push('\n');
    for row in &fixture.rows {
        write_record(
            &mut out,
            &[
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
            ],
        );
    }
    assert_no_long_digit_run("csv", &out);
    out
}

pub fn render_coverage(fixture: &Fixture) -> String {
    let rows = &fixture.rows;
    let neg = rows
        .iter()
        .filter(|row| parse_amount(&row.amount).expect("amount") < 0)
        .count();
    let pos = rows.len() - neg;
    let transfer = rows.iter().filter(|row| is_transfer(&row.category)).count();
    let notes = rows.iter().filter(|row| !row.notes.is_empty()).count();
    let untagged = rows.iter().filter(|row| row.tags.is_empty()).count();
    let mut accounts: Vec<&str> = rows.iter().map(|row| row.account.as_str()).collect();
    accounts.sort_unstable();
    accounts.dedup();
    let mut tags: Vec<&str> = rows
        .iter()
        .filter(|row| !row.tags.is_empty())
        .map(|row| row.tags.as_str())
        .collect();
    tags.sort_unstable();
    tags.dedup();
    let mut categories: Vec<&str> = rows.iter().map(|row| row.category.as_str()).collect();
    categories.sort_unstable();
    categories.dedup();
    let owner = rows.first().map(|row| row.owner.as_str()).unwrap_or("");

    let mut out = String::new();
    out.push_str("# synthetic October 2025 Monarch fixture\n\n");
    out.push_str(
        "Invented Monarch Money export for October 2025. Merchants, notes, and ids are fictional. Account names show at most a last-4.\n\n",
    );
    out.push_str(
        "`monarch-synth` writes `october-2025-synthetic.csv` beside this file. From `accounting-core`:\n\n",
    );
    out.push_str("```\ncargo run -p monarch-synth -- --check\n```\n\n");
    out.push_str(
        "`--check` confirms the committed CSV and this file match the default seed (`DEFAULT_SEED`, the digits of 2025-10-15). `--seed` changes filler merchants, amounts, and days. Scenario rows, and the ids in the table below, stay put.\n\n",
    );
    out.push_str("Every `Date` is in October 2025 (2025-10-01 through 2025-10-31). Amounts are signed: negative is an outflow, positive is an inflow, and none are zero. Every row has `Pending` false, `Reviewed` true, a `Reviewed At` timestamp, and a `Reviewed By` value.\n\n");
    out.push_str("## Columns\n\n");
    out.push_str(&format!("`{}`\n\n", COLUMNS.join(",")));
    out.push_str("## Shape\n\n");
    out.push_str(&format!("- Data rows: {}\n", rows.len()));
    out.push_str(&format!("- Distinct accounts: {}\n", accounts.len()));
    out.push_str(&format!("- Distinct categories: {}\n", categories.len()));
    out.push_str(&format!(
        "- Negative amounts: {} percent\n",
        percent(neg, rows.len())
    ));
    out.push_str(&format!(
        "- Positive amounts: {} percent\n",
        percent(pos, rows.len())
    ));
    out.push_str(&format!(
        "- Transfer or payment categories: {} percent\n",
        percent(transfer, rows.len())
    ));
    out.push_str(&format!("- Distinct e-tags: {}\n", tags.len()));
    out.push_str(&format!("- Untagged rows: {}\n", untagged));
    out.push_str(&format!("- Rows with notes: {}\n", notes));
    out.push_str(&format!("- Owner: {owner}\n\n"));
    out.push_str("## Accounts\n\n");
    for account in &accounts {
        out.push_str(&format!("- {account}\n"));
    }
    out.push('\n');
    out.push_str("## Tags\n\n");
    for tag in &tags {
        out.push_str(&format!("- {tag}\n"));
    }
    out.push('\n');
    out.push_str("## Scenario coverage\n\n");
    out.push_str("The ids below are the rows that exercise each scenario. Filler rows use the same columns and are not listed here.\n\n");
    out.push_str("| Scenario | Ids | What the rows show |\n");
    out.push_str("| --- | --- | --- |\n");
    for scenario in &fixture.scenarios {
        assert!(
            !scenario.name.contains('|') && !scenario.evidence.contains('|'),
            "coverage cell contains a pipe"
        );
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            scenario.name,
            scenario.ids.join(", "),
            scenario.evidence
        ));
    }
    assert_no_long_digit_run("coverage", &out);
    out
}

pub fn parse_csv(text: &str) -> Result<Vec<Row>, String> {
    let rest = text
        .strip_prefix(MARKER)
        .ok_or_else(|| "missing # synthetic marker".to_string())?;
    let rest = rest
        .strip_prefix('\n')
        .ok_or_else(|| "marker must be its own first line".to_string())?;
    let records = split_records(rest)?;
    if records.is_empty() {
        return Err("missing header".into());
    }
    if records[0].len() != COLUMNS.len()
        || records[0]
            .iter()
            .zip(COLUMNS)
            .any(|(field, column)| field != column)
    {
        return Err(format!("unexpected header: {:?}", records[0]));
    }
    records
        .iter()
        .skip(1)
        .map(|fields| row_from_fields(fields))
        .collect()
}

pub fn parse_amount(amount: &str) -> Result<i64, String> {
    let (negative, rest) = match amount.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, amount),
    };
    let (dollars, cents) = rest
        .split_once('.')
        .ok_or_else(|| format!("amount {amount} needs decimals"))?;
    if dollars.is_empty()
        || dollars.len() > 4
        || !dollars.bytes().all(|byte| byte.is_ascii_digit())
        || cents.len() != 2
        || !cents.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("amount {amount} is outside the fixture format"));
    }
    let dollars: i64 = dollars
        .parse()
        .map_err(|_| format!("amount {amount} is outside the fixture format"))?;
    let cents: i64 = cents
        .parse()
        .map_err(|_| format!("amount {amount} is outside the fixture format"))?;
    let value = dollars * 100 + cents;
    Ok(if negative { -value } else { value })
}

pub fn format_amount(cents: i64) -> String {
    assert!(cents != 0, "amount must be non-zero");
    let abs = cents.abs();
    assert!(
        abs / 100 <= 9_999,
        "amount {cents} has more than 4 dollar digits"
    );
    let body = format!("{}.{:02}", abs / 100, abs % 100);
    if cents < 0 {
        format!("-{body}")
    } else {
        body
    }
}

fn percent(part: usize, whole: usize) -> usize {
    part * 100 / whole
}

fn is_transfer(category: &str) -> bool {
    TRANSFER.contains(&category)
}

fn category_allowed(category: &str) -> bool {
    is_transfer(category) || INCOME.contains(&category) || EXPENSE.contains(&category)
}

fn assert_catalog_disjoint() {
    for category in EXPENSE {
        assert!(
            !is_transfer(category) && !INCOME.contains(&category),
            "{category} is in two catalogs"
        );
    }
    for category in INCOME {
        assert!(!is_transfer(category), "{category} is in two catalogs");
    }
}

fn reserved_amounts() -> Vec<i64> {
    [
        AMT_GROCER,
        AMT_COFFEE,
        AMT_PAPER,
        AMT_CARD_PAYMENT,
        AMT_PAYCHECK,
        AMT_HSA,
        AMT_DRAGONFLY,
        AMT_WORKSHOP,
        AMT_REIMBURSE,
        AMT_REFUND,
        AMT_PERK,
        AMT_DINER,
        AMT_INFLIGHT,
    ]
    .into_iter()
    .map(i64::abs)
    .collect()
}

fn canonical_rows() -> Vec<Row> {
    vec![
        make_row(
            "s-card-01",
            "2025-10-02",
            GROCER,
            "Groceries",
            CINDER_VISA,
            "POS MOTH AND MAPLE GROCER REF 1102",
            "",
            AMT_GROCER,
            "e-household",
        ),
        make_row(
            "s-card-02",
            "2025-10-06",
            "Kettlewhistle Coffee (fictional)",
            "Coffee Shops",
            FABLE,
            "POS KETTLEWHISTLE COFFEE REF 1106",
            "",
            AMT_COFFEE,
            "e-household",
        ),
        make_row(
            "s-card-03",
            "2025-10-11",
            "Paperkite Supplies (fictional)",
            "Shopping",
            NIMBUS,
            "POS PURCHASE PAPERKITE SUPPLIES\nSAMPLE CITY REF 4410",
            "",
            AMT_PAPER,
            "e-household",
        ),
        make_row(
            "s-cc-out",
            "2025-10-20",
            "Cinder Visa Payment (fictional)",
            "Credit Card Payment",
            PEBBLE_CHECKING,
            "ACH DEBIT CARD PAYMENT REF 2020",
            "",
            -AMT_CARD_PAYMENT,
            "e-household",
        ),
        make_row(
            "s-cc-in",
            "2025-10-20",
            "Cinder Visa Payment (fictional)",
            "Credit Card Payment",
            CINDER_VISA,
            "ACH CREDIT CARD PAYMENT REF 2021",
            "",
            AMT_CARD_PAYMENT,
            "e-household",
        ),
        make_row(
            "s-pay-01",
            "2025-10-03",
            "Numetric Payroll (fictional)",
            "Paychecks",
            PEBBLE_CHECKING,
            "PAYROLL DEPOSIT NUMETRIC REF 1003",
            "",
            AMT_PAYCHECK,
            "e-numetric",
        ),
        make_row(
            "s-pay-02",
            "2025-10-17",
            "Numetric Payroll (fictional)",
            "Paychecks",
            PEBBLE_CHECKING,
            "PAYROLL DEPOSIT NUMETRIC REF 1017",
            "",
            AMT_PAYCHECK,
            "e-numetric",
        ),
        make_row(
            "s-hsa-01",
            "2025-10-03",
            "HSA Contribution (fictional)",
            "HSA Contribution",
            PEBBLE_CHECKING,
            "ACH DEBIT HSA CONTRIBUTION REF 1250",
            "Synthetic: HSA contribution leaves checking in this file.\nThe matching HSA deposit is missing.",
            AMT_HSA,
            "e-household",
        ),
        make_row(
            "s-dfly-01",
            "2025-10-08",
            "Pineboard Lumber (fictional)",
            "Home Improvement",
            CINDER_MC,
            "POS PINEBOARD LUMBER REF 1008",
            "Synthetic: Dragonfly Homes jobsite materials, paid on a household card.",
            AMT_DRAGONFLY,
            "e-dragonfly",
        ),
        make_row(
            "s-exp-01",
            "2025-10-09",
            "Lantern Workshop (fictional)",
            "Education",
            CINDER_VISA,
            "POS LANTERN WORKSHOP REF 1009",
            "Synthetic: Numetric-tagged workshop fee.",
            AMT_WORKSHOP,
            "e-numetric",
        ),
        make_row(
            "s-reimb-01",
            "2025-10-22",
            "Numetric Reimburse (fictional)",
            "Reimbursements",
            PEBBLE_CHECKING,
            "ACH CREDIT REIMBURSE REF 1022",
            "Synthetic: partial Numetric reimbursement. Remainder stays open.",
            AMT_REIMBURSE,
            "e-numetric",
        ),
        make_row(
            "s-refund-01",
            "2025-10-14",
            GROCER,
            "Groceries",
            CINDER_VISA,
            "RETURN MOTH AND MAPLE GROCER REF 1014",
            "",
            AMT_REFUND,
            "e-household",
        ),
        make_row(
            "s-perk-01",
            "2025-10-18",
            "Cinder Perk Credit (fictional)",
            "Subscriptions",
            CINDER_VISA,
            "STATEMENT CREDIT PERK REF 1018",
            "Synthetic: statement credit, \"perk\" offset to Subscriptions.",
            AMT_PERK,
            "e-household",
        ),
        make_row(
            "s-dup-a",
            "2025-10-12",
            DINER,
            "Restaurants & Bars",
            FABLE,
            "POS BLUEFERN DINER REF 2315",
            "",
            AMT_DINER,
            "e-household",
        ),
        make_row(
            "s-dup-b",
            "2025-10-12",
            DINER,
            "Restaurants & Bars",
            FABLE,
            "POS BLUEFERN DINER REF 2315",
            "",
            AMT_DINER,
            "e-household",
        ),
        make_row(
            "s-fly-01",
            "2025-10-31",
            "Savings Transfer (fictional)",
            "Savings Transfer",
            PEBBLE_CHECKING,
            "ACH DEBIT SAVINGS TRANSFER REF 1031",
            "Synthetic: in-flight transfer. The other side posts in November and is absent from this file.",
            AMT_INFLIGHT,
            "e-household",
        ),
    ]
}

fn scenario_catalog() -> Vec<Scenario> {
    vec![
        scenario(
            "Card buys",
            &["s-card-01", "s-card-02", "s-card-03"],
            "Negative purchases on fictional credit cards.",
        ),
        scenario(
            "Credit-card payment (both sides)",
            &["s-cc-out", "s-cc-in"],
            "Checking outflow and card inflow on the same date with opposite signs, both Credit Card Payment.",
        ),
        scenario(
            "Paycheck",
            &["s-pay-01", "s-pay-02"],
            "Positive Paychecks rows from Numetric Payroll (fictional), tagged e-numetric.",
        ),
        scenario(
            "HSA contribution (matching side missing)",
            &["s-hsa-01"],
            "Checking outflow in HSA Contribution. The HSA deposit side is absent from this file.",
        ),
        scenario(
            "Dragonfly e-tagged buy",
            &["s-dfly-01"],
            "Negative e-dragonfly purchase for Dragonfly Homes on a household credit card.",
        ),
        scenario(
            "Numetric partial reimbursement",
            &["s-exp-01", "s-reimb-01"],
            "e-numetric expense paired with a smaller e-numetric Reimbursements inflow. The note says the reimbursement is partial and the remainder stays open.",
        ),
        scenario(
            "Refund",
            &["s-refund-01"],
            "Positive Groceries amount on a card, original statement contains RETURN, merchant matches an earlier buy.",
        ),
        scenario(
            "Card perk or statement credit",
            &["s-perk-01"],
            "Positive Subscriptions amount on a card whose original statement contains STATEMENT CREDIT.",
        ),
        scenario(
            "Duplicate row",
            &["s-dup-a", "s-dup-b"],
            "These two rows share every field except Id.",
        ),
        scenario(
            "In-flight item",
            &["s-fly-01"],
            "Late-October transfer outflow. The note says the other side posts in November and that side is absent from this file.",
        ),
    ]
}

fn scenario(name: &'static str, ids: &[&str], evidence: &'static str) -> Scenario {
    assert!(!evidence.contains('\n'), "{name} evidence must be one line");
    Scenario {
        name,
        ids: ids.iter().map(|id| (*id).to_string()).collect(),
        evidence,
    }
}

fn filler_rows(seed: u64) -> Vec<Row> {
    let mut build = Build::new(seed);
    for spec in transfer_pairs() {
        build.push_pair(&spec);
    }
    for index in 0..10 {
        build.push_outside_transfer(index);
    }
    for index in 0..26 {
        build.push_income(index);
    }
    let intro = intro_expenses();
    for (index, category) in intro.iter().enumerate() {
        build.push_spend(category, index == 0, false);
    }
    for index in 0..108 {
        let category = EXPENSE[index % EXPENSE.len()];
        build.push_spend(category, false, index < 2);
    }
    assert_eq!(build.draft.seq, 184, "filler row count drifted");
    build.rows
}

struct PairSpec {
    category: &'static str,
    from: &'static str,
    to: &'static str,
    merchant: &'static str,
}

fn transfer_pairs() -> [PairSpec; 8] {
    [
        PairSpec {
            category: "Savings Transfer",
            from: PEBBLE_CHECKING,
            to: PEBBLE_SAVINGS,
            merchant: "Savings Transfer (fictional)",
        },
        PairSpec {
            category: "Savings Transfer",
            from: HARBOR_JOINT,
            to: PEBBLE_SAVINGS,
            merchant: "Savings Transfer (fictional)",
        },
        PairSpec {
            category: "Investment Transfer",
            from: PEBBLE_CHECKING,
            to: BRAMBLE,
            merchant: "Brokerage Transfer (fictional)",
        },
        PairSpec {
            category: "Investment Transfer",
            from: LUMEN_CASH,
            to: BRAMBLE,
            merchant: "Brokerage Transfer (fictional)",
        },
        PairSpec {
            category: "Loan Payment",
            from: PEBBLE_CHECKING,
            to: OAK,
            merchant: "Oak Mortgage Payment (fictional)",
        },
        PairSpec {
            category: "Loan Payment",
            from: HARBOR_JOINT,
            to: SABLE,
            merchant: "Sable Auto Payment (fictional)",
        },
        PairSpec {
            category: "Credit Card Payment",
            from: PEBBLE_CHECKING,
            to: CINDER_MC,
            merchant: "Card Payment (fictional)",
        },
        PairSpec {
            category: "Credit Card Payment",
            from: LUMEN_CASH,
            to: FABLE,
            merchant: "Card Payment (fictional)",
        },
    ]
}

fn intro_expenses() -> Vec<&'static str> {
    const USED: &[&str] = &[
        "Groceries",
        "Restaurants & Bars",
        "Coffee Shops",
        "Home Improvement",
        "Shopping",
        "Subscriptions",
        "Education",
    ];
    let intro: Vec<&str> = EXPENSE
        .into_iter()
        .filter(|category| !USED.contains(category))
        .collect();
    assert_eq!(intro.first().copied(), Some("Gas"));
    assert_eq!(intro.len(), 24, "intro expenses drifted: {intro:?}");
    intro
}

struct Draft {
    rng: Rng,
    reserved: Vec<i64>,
    used: Vec<i64>,
    seq: u32,
    tag_cursor: usize,
    card_cursor: usize,
    bill_cursor: usize,
}

impl Draft {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let tag_cursor = (rng.next_u32() as usize) % TAGS.len();
        Self {
            rng,
            reserved: reserved_amounts(),
            used: Vec::new(),
            seq: 0,
            tag_cursor,
            card_cursor: 0,
            bill_cursor: 0,
        }
    }

    fn next_id(&mut self) -> (String, u32) {
        self.seq += 1;
        (format!("sf{:03}", self.seq), self.seq % 10_000)
    }

    fn next_tag(&mut self) -> String {
        let tag = TAGS[self.tag_cursor % TAGS.len()];
        self.tag_cursor += 1;
        tag.to_string()
    }

    fn day(&mut self) -> String {
        let day = 1 + self.rng.next_u32() % 31;
        format!("2025-10-{day:02}")
    }

    fn cents(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u32::try_from(hi - lo + 1).expect("amount band fits u32");
        for _ in 0..10_000 {
            let value = lo + i64::from(self.rng.next_u32() % span);
            if value > 0 && !self.reserved.contains(&value) && !self.used.contains(&value) {
                self.used.push(value);
                return value;
            }
        }
        panic!("amount space exhausted for {lo}..={hi}");
    }

    fn merchant(&mut self) -> String {
        let adjective = ADJECTIVES[(self.rng.next_u32() as usize) % ADJECTIVES.len()];
        let noun = NOUNS[(self.rng.next_u32() as usize) % NOUNS.len()];
        format!("{adjective} {noun} (fictional)")
    }
}

struct Build {
    draft: Draft,
    rows: Vec<Row>,
}

impl Build {
    fn new(seed: u64) -> Self {
        Self {
            draft: Draft::new(seed),
            rows: Vec::with_capacity(184),
        }
    }

    fn push_pair(&mut self, spec: &PairSpec) {
        let day = self.draft.day();
        let cents = self.draft.cents(2_500, 180_000);
        let tag = self.draft.next_tag();
        let (id_out, ref_out) = self.draft.next_id();
        let (id_in, ref_in) = self.draft.next_id();
        self.rows.push(make_row(
            &id_out,
            &day,
            spec.merchant,
            spec.category,
            spec.from,
            &format!("ACH DEBIT {} REF {ref_out:04}", spec.category),
            "",
            -cents,
            &tag,
        ));
        self.rows.push(make_row(
            &id_in,
            &day,
            spec.merchant,
            spec.category,
            spec.to,
            &format!("ACH CREDIT {} REF {ref_in:04}", spec.category),
            "",
            cents,
            &tag,
        ));
    }

    fn push_outside_transfer(&mut self, index: usize) {
        let day = self.draft.day();
        let cents = self.draft.cents(2_500, 180_000);
        let tag = self.draft.next_tag();
        let (id, reference) = self.draft.next_id();
        self.rows.push(make_row(
            &id,
            &day,
            "Outside Transfer (fictional)",
            "Transfer",
            CHECKING[index % CHECKING.len()],
            &format!("ACH DEBIT OUTSIDE TRANSFER REF {reference:04}"),
            "",
            -cents,
            &tag,
        ));
    }

    fn push_income(&mut self, index: usize) {
        let category = INCOME_FILLER[index % INCOME_FILLER.len()];
        let (lo, hi) = income_band(category);
        let day = self.draft.day();
        let cents = self.draft.cents(lo, hi);
        let tag = self.draft.next_tag();
        let (id, reference) = self.draft.next_id();
        let merchant = self.draft.merchant();
        self.rows.push(make_row(
            &id,
            &day,
            &merchant,
            category,
            income_account(category),
            &format!("ACH CREDIT {category} REF {reference:04}"),
            "",
            cents,
            &tag,
        ));
    }

    fn push_spend(&mut self, category: &str, dragonfly: bool, untagged: bool) {
        let (lo, hi) = if is_bill(category) {
            (8_000, 250_000)
        } else {
            (500, 18_999)
        };
        let day = self.draft.day();
        let cents = self.draft.cents(lo, hi);
        let tag = if untagged {
            String::new()
        } else if dragonfly {
            "e-dragonfly".to_string()
        } else {
            self.draft.next_tag()
        };
        let (id, reference) = self.draft.next_id();
        let merchant = self.draft.merchant();
        let account = if dragonfly {
            DRAGONFLY
        } else if is_bill(category) {
            let account = CHECKING[self.draft.bill_cursor % CHECKING.len()];
            self.draft.bill_cursor += 1;
            account
        } else {
            let account = CARDS[self.draft.card_cursor % CARDS.len()];
            self.draft.card_cursor += 1;
            account
        };
        self.rows.push(make_row(
            &id,
            &day,
            &merchant,
            category,
            account,
            &format!("POS {merchant} REF {reference:04}"),
            "",
            -cents,
            &tag,
        ));
    }
}

const INCOME_FILLER: [&str; 5] = [
    "Interest",
    "Dividends",
    "Other Income",
    "Business Income",
    "Gifts Received",
];

fn income_band(category: &str) -> (i64, i64) {
    match category {
        "Interest" => (50, 899),
        "Dividends" => (500, 20_000),
        "Business Income" => (5_000, 400_000),
        _ => (1_000, 80_000),
    }
}

fn income_account(category: &str) -> &'static str {
    match category {
        "Interest" => PEBBLE_SAVINGS,
        "Dividends" => BRAMBLE,
        "Business Income" => DRAGONFLY,
        "Gifts Received" => HARBOR_JOINT,
        "Other Income" => PEBBLE_CHECKING,
        _ => panic!("unexpected filler income category {category}"),
    }
}

fn is_bill(category: &str) -> bool {
    matches!(
        category,
        "Rent"
            | "Mortgage"
            | "Utilities"
            | "Insurance"
            | "Phone"
            | "Internet & Cable"
            | "Child Care"
            | "Taxes"
            | "Fees"
    )
}

#[allow(clippy::too_many_arguments)]
fn make_row(
    id: &str,
    date: &str,
    merchant: &str,
    category: &str,
    account: &str,
    original_statement: &str,
    notes: &str,
    amount_cents: i64,
    tags: &str,
) -> Row {
    assert!(!id.is_empty(), "id is empty");
    assert_october_2025(date);
    assert!(
        ACCOUNTS.contains(&account),
        "unknown account {account} on {id}"
    );
    assert!(
        category_allowed(category),
        "unknown category {category} on {id}"
    );
    if !tags.is_empty() {
        assert!(TAGS.contains(&tags), "unknown tag {tags} on {id}");
    }
    let amount = format_amount(amount_cents);
    let row = Row {
        date: date.to_string(),
        merchant: merchant.to_string(),
        category: category.to_string(),
        account: account.to_string(),
        original_statement: original_statement.to_string(),
        notes: notes.to_string(),
        amount,
        tags: tags.to_string(),
        owner: OWNER.to_string(),
        reviewed: "true".to_string(),
        reviewed_at: reviewed_at(date, merchant, amount_cents),
        reviewed_by: REVIEWED_BY.to_string(),
        pending: "false".to_string(),
        id: id.to_string(),
    };
    for (label, value) in [
        ("date", row.date.as_str()),
        ("merchant", row.merchant.as_str()),
        ("category", row.category.as_str()),
        ("account", row.account.as_str()),
        ("statement", row.original_statement.as_str()),
        ("notes", row.notes.as_str()),
        ("amount", row.amount.as_str()),
        ("tags", row.tags.as_str()),
        ("owner", row.owner.as_str()),
        ("reviewed", row.reviewed.as_str()),
        ("reviewed_at", row.reviewed_at.as_str()),
        ("reviewed_by", row.reviewed_by.as_str()),
        ("pending", row.pending.as_str()),
        ("id", row.id.as_str()),
    ] {
        assert_no_long_digit_run(&format!("{id} {label}"), value);
    }
    row
}

fn reviewed_at(date: &str, merchant: &str, amount_cents: i64) -> String {
    let mut hash: u32 = 17;
    for byte in merchant.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(u32::from(byte));
    }
    hash ^= amount_cents as u32;
    let hour = 8 + (hash % 11);
    let minute = (hash / 11) % 60;
    format!("{date} {hour:02}:{minute:02}:00")
}

fn assert_october_2025(date: &str) {
    let Some(day) = date.strip_prefix("2025-10-") else {
        panic!("date {date} is outside October 2025");
    };
    assert_eq!(day.len(), 2, "{date}");
    let day: u32 = day.parse().unwrap_or_else(|_| panic!("date {date}"));
    assert!((1..=31).contains(&day), "{date}");
}

fn check_shape(rows: &[Row], scenarios: &[Scenario]) {
    assert_eq!(rows.len(), 200, "row count");
    assert_eq!(scenarios.len(), 10, "scenario count");
    for scenario in scenarios {
        assert!(!scenario.ids.is_empty(), "{}", scenario.name);
        for id in &scenario.ids {
            assert!(
                rows.iter().any(|row| row.id == *id),
                "scenario {} missing {id}",
                scenario.name
            );
        }
    }

    let mut ids = BTreeSet::new();
    let mut accounts = BTreeSet::new();
    let mut categories = BTreeSet::new();
    let mut tags = BTreeSet::new();
    let mut negative = 0;
    let mut positive = 0;
    let mut transfer = 0;
    let mut untagged = 0;
    let mut noted = 0;
    let mut multiline = 0;
    let mut content_counts: Vec<(String, String)> = Vec::new();

    for row in rows {
        assert!(ids.insert(row.id.clone()), "duplicate id {}", row.id);
        accounts.insert(row.account.as_str());
        categories.insert(row.category.as_str());
        if row.tags.is_empty() {
            untagged += 1;
        } else {
            tags.insert(row.tags.as_str());
        }
        if !row.notes.is_empty() {
            noted += 1;
        }
        if row.original_statement.contains('\n') || row.notes.contains('\n') {
            multiline += 1;
        }
        let cents = parse_amount(&row.amount).expect("amount");
        if cents < 0 {
            negative += 1;
        } else {
            positive += 1;
        }
        if is_transfer(&row.category) {
            transfer += 1;
        }
        assert_eq!(row.owner, OWNER);
        assert_eq!(row.reviewed, "true");
        assert_eq!(row.pending, "false");
        assert_eq!(row.reviewed_by, REVIEWED_BY);
        assert!(!row.reviewed_at.is_empty());
        content_counts.push((content_key(row), row.id.clone()));
    }

    assert_eq!(negative, 160, "negative rows");
    assert_eq!(positive, 40, "positive rows");
    assert_eq!(transfer, 30, "transfer or payment rows");
    assert_eq!(untagged, 2, "untagged rows");
    assert_eq!(noted, 6, "rows with notes");
    assert!(multiline >= 2, "multiline fields");
    assert_eq!(accounts.len(), ACCOUNTS.len());
    for account in ACCOUNTS {
        assert!(accounts.contains(account), "missing account {account}");
    }
    assert_eq!(categories.len(), 44, "category count");
    for category in TRANSFER.into_iter().chain(INCOME).chain(EXPENSE) {
        assert!(categories.contains(category), "missing category {category}");
    }
    assert_eq!(tags.len(), TAGS.len());
    for tag in TAGS {
        assert!(tags.contains(tag), "missing tag {tag}");
    }

    content_counts.sort();
    let mut duplicate_ids = Vec::new();
    let mut index = 0;
    while index < content_counts.len() {
        let mut end = index + 1;
        while end < content_counts.len() && content_counts[end].0 == content_counts[index].0 {
            end += 1;
        }
        if end - index > 1 {
            duplicate_ids.push(
                content_counts[index..end]
                    .iter()
                    .map(|(_, id)| id.clone())
                    .collect::<Vec<_>>(),
            );
        }
        index = end;
    }
    assert_eq!(
        duplicate_ids,
        vec![vec!["s-dup-a".to_string(), "s-dup-b".to_string()]],
        "unexpected duplicate content"
    );

    let hsa = row_by_id(rows, "s-hsa-01");
    let hsa_cents = parse_amount(&hsa.amount).unwrap();
    assert!(hsa_cents < 0);
    assert!(rows
        .iter()
        .all(|row| parse_amount(&row.amount).unwrap() != -hsa_cents));
    assert!(rows
        .iter()
        .all(|row| !row.account.to_ascii_lowercase().contains("hsa")));

    let inflight = row_by_id(rows, "s-fly-01");
    let inflight_cents = parse_amount(&inflight.amount).unwrap();
    assert!(inflight.date.as_str() >= "2025-10-28");
    assert!(inflight_cents < 0);
    assert!(rows
        .iter()
        .all(|row| parse_amount(&row.amount).unwrap() != -inflight_cents));
}

fn row_by_id<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("missing {id}"))
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
                let from = start.saturating_sub(16);
                let to = (index + 16).min(bytes.len());
                let snippet = String::from_utf8_lossy(&bytes[from..to]);
                panic!("{label} contains a number longer than 4 digits near `{snippet}`");
            }
        } else {
            run = 0;
        }
    }
}

fn write_record(out: &mut String, fields: &[&str]) {
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_field(out, field);
    }
    out.push('\n');
}

fn write_field(out: &mut String, field: &str) {
    if field.contains([',', '"', '\n', '\r']) {
        out.push('"');
        for ch in field.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
    } else {
        out.push_str(field);
    }
}

fn split_records(body: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' => {
                if !field.is_empty() {
                    return Err("quote started mid-field".into());
                }
                in_quotes = true;
            }
            ',' => record.push(std::mem::take(&mut field)),
            '\n' => {
                record.push(std::mem::take(&mut field));
                if record.len() > 1 || record.iter().any(|item| !item.is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
            '\r' => return Err("CR is not used in this fixture format".into()),
            _ => field.push(ch),
        }
    }
    if in_quotes {
        return Err("unterminated quote".into());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

fn row_from_fields(fields: &[String]) -> Result<Row, String> {
    if fields.len() != COLUMNS.len() {
        return Err(format!(
            "expected {} columns, found {}",
            COLUMNS.len(),
            fields.len()
        ));
    }
    Ok(Row {
        date: fields[0].clone(),
        merchant: fields[1].clone(),
        category: fields[2].clone(),
        account: fields[3].clone(),
        original_statement: fields[4].clone(),
        notes: fields[5].clone(),
        amount: fields[6].clone(),
        tags: fields[7].clone(),
        owner: fields[8].clone(),
        reviewed: fields[9].clone(),
        reviewed_at: fields[10].clone(),
        reviewed_by: fields[11].clone(),
        pending: fields[12].clone(),
        id: fields[13].clone(),
    })
}

/// Small PCG32. Output is a pure function of the seed.
struct Rng {
    state: u64,
    increment: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: seed.wrapping_shl(1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.increment);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rotation = (old >> 59) as u32;
        xorshifted.rotate_right(rotation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_roundtrip_preserves_commas_quotes_and_newlines() {
        let row = make_row(
            "s-quote-01",
            "2025-10-04",
            "Quote Shop (fictional)",
            "Shopping",
            CINDER_VISA,
            "LINE ONE\nLINE TWO, STILL",
            "He said \"synthetic\", really",
            -1_234,
            "e-sample",
        );
        let fixture = Fixture {
            rows: vec![row],
            scenarios: Vec::new(),
        };
        let csv = render_csv(&fixture);
        assert!(csv.starts_with("# synthetic\n"));
        let parsed = parse_csv(&csv).unwrap();
        assert_eq!(parsed, fixture.rows);
        assert_eq!(parsed[0].original_statement, "LINE ONE\nLINE TWO, STILL");
        assert_eq!(parsed[0].notes, "He said \"synthetic\", really");
        assert_eq!(parsed[0].amount, "-12.34");
    }

    #[test]
    fn parse_rejects_a_file_without_the_marker() {
        let err = parse_csv("Date,Merchant\n").unwrap_err();
        assert!(err.contains("marker"), "{err}");
    }

    #[test]
    fn amount_format_roundtrip_stays_within_four_dollar_digits() {
        for cents in [-218_042, -12_500, -475, 1_299, 45_000, 40] {
            let text = format_amount(cents);
            assert_eq!(parse_amount(&text).unwrap(), cents);
            assert_no_long_digit_run("amount", &text);
        }
    }

    #[test]
    #[should_panic]
    fn format_amount_rejects_five_dollar_digits() {
        let _ = format_amount(1_000_000);
    }
}
