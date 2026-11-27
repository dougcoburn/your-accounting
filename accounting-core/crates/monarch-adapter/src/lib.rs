//! Monarch CSV feed (U23).
//!
//! Translates a Monarch export into observation rows and writes them through
//! the observation import context (`import_observations`). The kernel learns
//! no Monarch column. `Id` is the external id. The Date column is both the
//! occurred date and the posted date. Reviewed is stored as a hint and does
//! not reconcile the row. Pending rows are not stored, so a later posted
//! export can insert under the same id. A row whose hash changed is reported
//! by file line and column name and is not replaced.
//!
//! Category names in the caller-supplied class list become leaf ledger
//! accounts under an Income or Expense parent. The adapter does not create
//! financial accounts: the local account map is `financial_accounts.last4`.
//! It does not write `tags` or `posting_tags`.
//!
//! The kernel must be opened with the observation-import participant. Commit
//! runs that participant's `validate`. Linking and voiding stay on the
//! transaction (`observation_links`, `void_observation`); this feed does not
//! post events.

mod csv;
mod error;
mod row;
mod seed;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use accounting_kernel::v2::{
    AccountType, Code, Error as KernelError, ImportBatch, Kernel, LedgerAccountRow, LedgerKind,
    NewFeed, NewLedgerAccount, ObservationInput, Transaction,
};

pub use error::{ColumnDiff, ImportError, RowFault, COLUMNS};
pub use seed::{month1_category_classes, CategoryClass, EXPENSE_PARENT, INCOME_PARENT};

pub const IMPORTER_ID: &str = "monarch_csv";
pub const IMPORTER_VERSION: &str = "1";

/// One import of a Monarch export into an entity database that already has
/// its financial accounts.
pub struct ImportRequest<'a> {
    pub csv: &'a str,
    pub feed_id: &'a str,
    pub feed_name: &'a str,
    pub expected_revision: i64,
    pub categories: &'a [CategoryClass],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub feed_id: String,
    pub data_rows: u32,
    pub inserted: u32,
    pub unchanged: u32,
    /// Physical lines of pending rows that were not stored.
    pub withheld_pending: Vec<u32>,
    /// Same external id, different column text. The stored observation is unchanged.
    pub proposals: Vec<ColumnDiff>,
    pub observation_ids: Vec<String>,
    pub leaves_created: u32,
    pub noop: bool,
    pub seq: i64,
}

struct ResolvedAccount {
    id: String,
    currency: String,
}

/// Import `request.csv`. A parse or account fault writes nothing.
///
/// Pending rows with no stored observation are withheld. Rows whose stored
/// hash differs are left in place and listed on `proposals`; every other
/// accepted row is written in the same commit.
pub fn import_monarch_csv(
    kernel: &mut Kernel,
    request: &ImportRequest<'_>,
) -> Result<ImportReport, ImportError> {
    let sources = read_sources(request.csv)?;
    let data_rows = u32::try_from(sources.len()).unwrap_or(u32::MAX);
    let accounts = resolve_accounts(kernel, &sources)?;
    let Classified {
        batch,
        withheld_pending,
        proposals,
    } = classify(kernel, request.feed_id, &sources)?;
    if batch.is_empty() {
        return Ok(ImportReport {
            feed_id: request.feed_id.to_string(),
            data_rows,
            inserted: 0,
            unchanged: 0,
            withheld_pending,
            proposals,
            observation_ids: Vec::new(),
            leaves_created: 0,
            noop: true,
            seq: request.expected_revision,
        });
    }
    let classes = class_map(request.categories);
    let mut tx = kernel
        .begin_transaction(Some(request.expected_revision))
        .map_err(ImportError::kernel)?;
    let outcome = match write_batch(&mut tx, request, &classes, &accounts, &batch) {
        Ok(outcome) => outcome,
        Err(err) => return Err(err),
    };
    let sealed = tx.commit().map_err(ImportError::kernel)?;
    let unchanged = u32::try_from(outcome.observation_ids.len())
        .unwrap_or(u32::MAX)
        .saturating_sub(outcome.inserted);
    Ok(ImportReport {
        feed_id: request.feed_id.to_string(),
        data_rows,
        inserted: outcome.inserted,
        unchanged,
        withheld_pending,
        proposals,
        observation_ids: outcome.observation_ids,
        leaves_created: outcome.leaves_created,
        noop: sealed.noop,
        seq: sealed.seq,
    })
}

struct WriteOutcome {
    inserted: u32,
    observation_ids: Vec<String>,
    leaves_created: u32,
}

fn write_batch(
    tx: &mut Transaction<'_>,
    request: &ImportRequest<'_>,
    classes: &HashMap<&str, AccountType>,
    accounts: &HashMap<String, ResolvedAccount>,
    batch: &[&row::SourceRow],
) -> Result<WriteOutcome, ImportError> {
    tx.ensure_feed(NewFeed {
        id: Some(request.feed_id.to_string()),
        kind: "monarch_csv".to_string(),
        name: request.feed_name.to_string(),
    })
    .map_err(ImportError::kernel)?;
    let leaves_created = seed_categories(tx, classes, batch)?;
    let mut rows = Vec::with_capacity(batch.len());
    for source in batch {
        let Some(account) = accounts.get(&source.last4) else {
            return Err(ImportError::rows(vec![RowFault {
                row: source.line,
                column: "Account",
            }]));
        };
        rows.push(ObservationInput {
            id: None,
            external_id: source.external_id.clone(),
            financial_account_id: account.id.clone(),
            occurred_date: source.occurred_date.clone(),
            posted_date: Some(source.occurred_date.clone()),
            direction: source.direction.to_string(),
            amount_minor: source.amount_minor,
            currency: account.currency.clone(),
            description: source.description.clone(),
            row_hash: source.row_hash.clone(),
            payload_json: source.payload_json.clone(),
            corroborates_id: None,
        });
    }
    let imported = tx
        .import_observations(ImportBatch {
            feed_id: request.feed_id.to_string(),
            importer_id: IMPORTER_ID.to_string(),
            importer_version: IMPORTER_VERSION.to_string(),
            document_id: None,
            rows,
        })
        .map_err(|err| map_conflict(err, batch))?;
    Ok(WriteOutcome {
        inserted: imported.inserted,
        observation_ids: imported.observation_ids,
        leaves_created,
    })
}

fn map_conflict(err: KernelError, batch: &[&row::SourceRow]) -> ImportError {
    if err.code != Code::ImportConflict {
        return ImportError::kernel(err);
    }
    let Some(external_id) = err.ids.get(1) else {
        return ImportError::kernel(err);
    };
    if let Some(source) = batch
        .iter()
        .find(|source| &source.external_id == external_id)
    {
        return ImportError::rows(vec![RowFault {
            row: source.line,
            column: "Id",
        }]);
    }
    ImportError::kernel(err)
}

fn read_sources(csv: &str) -> Result<Vec<row::SourceRow>, ImportError> {
    let records = csv::parse_records(csv)?;
    let mut header = None;
    let mut data = Vec::new();
    for record in records {
        let comment = record.fields.len() == 1 && record.fields[0].starts_with('#');
        if header.is_none() && comment {
            continue;
        }
        if header.is_none() {
            header = Some(record);
            continue;
        }
        data.push(record);
    }
    let Some(header) = header else {
        return Err(ImportError::rows(vec![RowFault {
            row: 1,
            column: "Date",
        }]));
    };
    if header.fields.len() != COLUMNS.len()
        || header
            .fields
            .iter()
            .zip(COLUMNS)
            .any(|(field, column)| field != column)
    {
        let mut faults = Vec::new();
        for (index, column) in COLUMNS.iter().enumerate() {
            if header.fields.get(index).map(String::as_str) != Some(*column) {
                faults.push(RowFault {
                    row: header.line,
                    column,
                });
            }
        }
        if faults.is_empty() {
            faults.push(RowFault {
                row: header.line,
                column: "Id",
            });
        }
        return Err(ImportError::rows(faults));
    }
    let mut faults = Vec::new();
    let mut sources = Vec::new();
    for record in data {
        match row::interpret(record.line, &record.fields) {
            Ok(source) => sources.push(source),
            Err(row_faults) => faults.extend(row_faults),
        }
    }
    let mut seen = HashSet::new();
    for source in &sources {
        if !seen.insert(source.external_id.clone()) {
            faults.push(RowFault {
                row: source.line,
                column: "Id",
            });
        }
    }
    if !faults.is_empty() {
        faults.sort_by_key(|fault| (fault.row, fault.column));
        faults.dedup();
        return Err(ImportError::rows(faults));
    }
    Ok(sources)
}

fn resolve_accounts(
    kernel: &Kernel,
    sources: &[row::SourceRow],
) -> Result<HashMap<String, ResolvedAccount>, ImportError> {
    let mut resolved = HashMap::new();
    let mut faults = Vec::new();
    let mut last4s = BTreeSet::new();
    for source in sources {
        last4s.insert(source.last4.clone());
    }
    for last4 in last4s {
        let found = kernel
            .financial_accounts_with_last4(&last4)
            .map_err(ImportError::kernel)?;
        if found.len() != 1 || found[0].currency != "USD" {
            for source in sources.iter().filter(|source| source.last4 == last4) {
                faults.push(RowFault {
                    row: source.line,
                    column: "Account",
                });
            }
            continue;
        }
        resolved.insert(
            last4,
            ResolvedAccount {
                id: found[0].id.clone(),
                currency: found[0].currency.clone(),
            },
        );
    }
    if !faults.is_empty() {
        return Err(ImportError::rows(faults));
    }
    Ok(resolved)
}

struct Classified<'a> {
    batch: Vec<&'a row::SourceRow>,
    withheld_pending: Vec<u32>,
    proposals: Vec<ColumnDiff>,
}

fn classify<'a>(
    kernel: &Kernel,
    feed_id: &str,
    sources: &'a [row::SourceRow],
) -> Result<Classified<'a>, ImportError> {
    let mut batch = Vec::new();
    let mut withheld_pending = Vec::new();
    let mut proposals = Vec::new();
    for source in sources {
        let stored = kernel
            .observation_by_external_id(feed_id, &source.external_id)
            .map_err(ImportError::kernel)?;
        match stored {
            Some(fact) if fact.row_hash == source.row_hash => batch.push(source),
            Some(fact) => proposals.push(row::proposal(source, &fact.payload_json)),
            None if source.pending => withheld_pending.push(source.line),
            None => batch.push(source),
        }
    }
    Ok(Classified {
        batch,
        withheld_pending,
        proposals,
    })
}

fn class_map(categories: &[CategoryClass]) -> HashMap<&str, AccountType> {
    let mut classes = HashMap::new();
    for class in categories {
        classes
            .entry(class.name.as_str())
            .or_insert(class.account_type);
    }
    classes
}

fn seed_categories(
    tx: &mut Transaction<'_>,
    classes: &HashMap<&str, AccountType>,
    batch: &[&row::SourceRow],
) -> Result<u32, ImportError> {
    let mut names = BTreeMap::new();
    for source in batch {
        if classes.contains_key(source.category.as_str()) {
            names.entry(source.category.as_str()).or_insert(source.line);
        }
    }
    let mut parents = Parents::default();
    let mut created = 0u32;
    for (name, line) in names {
        let account_type = classes[name];
        let existing = tx
            .ledger_categories_named(name)
            .map_err(ImportError::kernel)?;
        if find_leaf(&existing).is_some() {
            continue;
        }
        let parent_id = ensure_parent(tx, &mut parents, account_type, line)?;
        tx.create_ledger_account(NewLedgerAccount {
            id: None,
            financial_account_id: None,
            account_type,
            kind: LedgerKind::Category,
            parent_id: Some(parent_id),
            name: name.to_string(),
        })
        .map_err(ImportError::kernel)?;
        created = created.saturating_add(1);
    }
    Ok(created)
}

#[derive(Default)]
struct Parents {
    income: Option<String>,
    expense: Option<String>,
}

fn ensure_parent(
    tx: &mut Transaction<'_>,
    parents: &mut Parents,
    account_type: AccountType,
    line: u32,
) -> Result<String, ImportError> {
    let slot = match account_type {
        AccountType::Income => &mut parents.income,
        AccountType::Expense => &mut parents.expense,
        _ => {
            return Err(ImportError::rows(vec![RowFault {
                row: line,
                column: "Category",
            }]))
        }
    };
    if let Some(id) = slot.clone() {
        return Ok(id);
    }
    let name = match account_type {
        AccountType::Income => INCOME_PARENT,
        AccountType::Expense => EXPENSE_PARENT,
        _ => INCOME_PARENT,
    };
    let existing = tx
        .ledger_categories_named(name)
        .map_err(ImportError::kernel)?;
    if let Some(id) = find_parent(&existing, account_type.as_str(), name) {
        *slot = Some(id.clone());
        return Ok(id);
    }
    let id = tx
        .create_ledger_account(NewLedgerAccount {
            id: None,
            financial_account_id: None,
            account_type,
            kind: LedgerKind::Category,
            parent_id: None,
            name: name.to_string(),
        })
        .map_err(ImportError::kernel)?;
    *slot = Some(id.clone());
    Ok(id)
}

fn find_parent(rows: &[LedgerAccountRow], account_type: &str, name: &str) -> Option<String> {
    rows.iter()
        .find(|row| {
            row.parent_id.is_none()
                && row.account_type == account_type
                && row.kind == "category"
                && row.name == name
        })
        .map(|row| row.id.clone())
}

fn find_leaf(rows: &[LedgerAccountRow]) -> Option<String> {
    rows.iter()
        .find(|row| row.kind == "category" && row.parent_id.is_some())
        .map(|row| row.id.clone())
}
