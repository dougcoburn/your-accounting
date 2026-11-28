//! Monarch CSV feed (U23).
//!
//! Translates a Monarch export into observation rows and writes them through
//! the observation import context. The kernel learns no Monarch column. With
//! a feed id, identity is that id plus the financial account. The raw-text
//! hash only detects change: the same hash is a no-op, and a different hash
//! appends a revision while the first snapshot stays. `Date` is both the
//! occurred date and the posted date. `Reviewed` and `Pending` are attributes
//! on the observation, not a reconciliation and not a reason to skip the row.
//! Category is a hint in the payload. This feed creates no ledger accounts
//! and no financial accounts, and it does not post kernel events.
//!
//! A later pull voids an observation only when that pull, for the same
//! account, covers at least seven days on either side of the observation's
//! date and the id is absent. A narrower export voids nothing. The void is
//! the import-context tombstone (commit kind `import`, no kernel reversal).
//!
//! The kernel must be opened with the observation-import participant.

mod csv;
mod dates;
mod error;
mod identity;
mod row;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use accounting_kernel::v2::{
    Code, Error as KernelError, ImportBatch, Kernel, NewFeed, ObservationInput,
};

pub use dates::Coverage;
pub use error::{ImportError, RowFault, COLUMNS};
pub use identity::{FeedIdStrategy, FeedIdentity, IdLessStrategy, IdentityError, IdentityStrategy};

pub const IMPORTER_ID: &str = "monarch_csv";
pub const IMPORTER_VERSION: &str = "1";

/// One import of a Monarch export. Financial accounts must already exist.
pub struct ImportRequest<'a> {
    pub csv: &'a str,
    pub feed_id: &'a str,
    pub feed_name: &'a str,
    pub expected_revision: i64,
}

/// A financial-movement observation already bound to one account.
pub struct FinancialObservation {
    pub external_id: String,
    pub occurred_date: String,
    pub posted_date: Option<String>,
    pub direction: String,
    pub amount_minor: i64,
    pub description: String,
    pub row_hash: String,
    pub payload_json: String,
}

/// App-layer entry. Every observation is for `financial_account_id`.
/// `coverage` is the pull's date span for that account. `None`, and an empty
/// list, void nothing.
pub struct AddObservations<'a> {
    pub financial_account_id: &'a str,
    pub feed_id: &'a str,
    pub feed_name: &'a str,
    pub expected_revision: i64,
    pub strategy: &'a dyn IdentityStrategy,
    pub observations: &'a [FinancialObservation],
    pub coverage: Option<Coverage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub feed_id: String,
    pub data_rows: u32,
    pub inserted: u32,
    pub revised: u32,
    pub unchanged: u32,
    pub voided: u32,
    pub observation_ids: Vec<String>,
    pub noop: bool,
    pub seq: i64,
}

struct ResolvedAccount {
    id: String,
    currency: String,
}

struct AccountScope {
    present: HashSet<String>,
    dates: Vec<String>,
}

/// Import `request.csv`. A parse fault or an account that is not exactly one
/// usable account writes nothing.
pub fn import_monarch_csv(
    kernel: &mut Kernel,
    request: &ImportRequest<'_>,
) -> Result<ImportReport, ImportError> {
    let sources = read_sources(request.csv)?;
    let data_rows = u32::try_from(sources.len()).unwrap_or(u32::MAX);
    if sources.is_empty() {
        return Ok(empty_report(
            request.feed_id,
            data_rows,
            request.expected_revision,
        ));
    }
    let accounts = resolve_accounts(kernel, &sources)?;
    let strategy = FeedIdStrategy;
    let mut rows = Vec::with_capacity(sources.len());
    let mut scopes: BTreeMap<String, AccountScope> = BTreeMap::new();
    for source in &sources {
        let account = &accounts[&source.last4];
        let identity = strategy
            .identify(&source.external_id, &account.id)
            .map_err(ImportError::Identity)?;
        let scope = scopes
            .entry(identity.financial_account_id.clone())
            .or_insert_with(|| AccountScope {
                present: HashSet::new(),
                dates: Vec::new(),
            });
        scope.present.insert(identity.external_id.clone());
        scope.dates.push(source.occurred_date.clone());
        rows.push(observation_input(
            identity,
            &account.currency,
            &FinancialObservation {
                external_id: source.external_id.clone(),
                occurred_date: source.occurred_date.clone(),
                posted_date: Some(source.occurred_date.clone()),
                direction: source.direction.to_string(),
                amount_minor: source.amount_minor,
                description: source.description.clone(),
                row_hash: source.row_hash.clone(),
                payload_json: source.payload_json.clone(),
            },
        ));
    }
    let voids = absent_ids(kernel, request.feed_id, &scopes)?;
    let mut tx = kernel
        .begin_transaction(Some(request.expected_revision))
        .map_err(ImportError::kernel)?;
    tx.ensure_feed(NewFeed {
        id: Some(request.feed_id.to_string()),
        kind: "monarch_csv".to_string(),
        name: request.feed_name.to_string(),
    })
    .map_err(ImportError::kernel)?;
    let imported = tx
        .import_observations(ImportBatch {
            feed_id: request.feed_id.to_string(),
            importer_id: IMPORTER_ID.to_string(),
            importer_version: IMPORTER_VERSION.to_string(),
            document_id: None,
            rows,
        })
        .map_err(ImportError::kernel)?;
    for id in &voids {
        tx.void_observation(id, "other")
            .map_err(ImportError::kernel)?;
    }
    let sealed = tx.commit().map_err(ImportError::kernel)?;
    Ok(finish(
        request.feed_id,
        &sealed,
        data_rows,
        &imported,
        voids.len(),
    ))
}

/// Store observations that are already bound to one financial account.
pub fn add_observations(
    kernel: &mut Kernel,
    request: &AddObservations<'_>,
) -> Result<ImportReport, ImportError> {
    let data_rows = u32::try_from(request.observations.len()).unwrap_or(u32::MAX);
    if request.observations.is_empty() {
        return Ok(empty_report(
            request.feed_id,
            data_rows,
            request.expected_revision,
        ));
    }
    let account = kernel
        .financial_account(request.financial_account_id)
        .map_err(ImportError::kernel)?
        .ok_or_else(|| ImportError::kernel(KernelError::new(Code::UnknownTarget, &[], vec![])))?;
    let mut seen = HashSet::new();
    let mut rows = Vec::with_capacity(request.observations.len());
    let mut present = HashSet::new();
    for observation in request.observations {
        let identity = request
            .strategy
            .identify(&observation.external_id, request.financial_account_id)
            .map_err(ImportError::Identity)?;
        if !seen.insert(identity.external_id.clone()) {
            return Err(ImportError::rows(vec![RowFault {
                row: 0,
                column: "Id",
            }]));
        }
        present.insert(identity.external_id.clone());
        rows.push(observation_input(identity, &account.currency, observation));
    }
    let voids = if let Some(coverage) = &request.coverage {
        let live = kernel
            .live_observations_for_account(request.feed_id, request.financial_account_id)
            .map_err(ImportError::kernel)?;
        live.into_iter()
            .filter(|obs| {
                !present.contains(&obs.external_id) && dates::covers(coverage, &obs.occurred_date)
            })
            .map(|obs| obs.id)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut tx = kernel
        .begin_transaction(Some(request.expected_revision))
        .map_err(ImportError::kernel)?;
    tx.ensure_feed(NewFeed {
        id: Some(request.feed_id.to_string()),
        kind: "monarch_csv".to_string(),
        name: request.feed_name.to_string(),
    })
    .map_err(ImportError::kernel)?;
    let imported = tx
        .import_observations(ImportBatch {
            feed_id: request.feed_id.to_string(),
            importer_id: IMPORTER_ID.to_string(),
            importer_version: IMPORTER_VERSION.to_string(),
            document_id: None,
            rows,
        })
        .map_err(ImportError::kernel)?;
    for id in &voids {
        tx.void_observation(id, "other")
            .map_err(ImportError::kernel)?;
    }
    let sealed = tx.commit().map_err(ImportError::kernel)?;
    Ok(finish(
        request.feed_id,
        &sealed,
        data_rows,
        &imported,
        voids.len(),
    ))
}

fn observation_input(
    identity: FeedIdentity,
    currency: &str,
    observation: &FinancialObservation,
) -> ObservationInput {
    ObservationInput {
        id: None,
        external_id: identity.external_id,
        financial_account_id: identity.financial_account_id,
        occurred_date: observation.occurred_date.clone(),
        posted_date: observation.posted_date.clone(),
        direction: observation.direction.clone(),
        amount_minor: observation.amount_minor,
        currency: currency.to_string(),
        description: observation.description.clone(),
        row_hash: observation.row_hash.clone(),
        payload_json: observation.payload_json.clone(),
        corroborates_id: None,
    }
}

fn absent_ids(
    kernel: &Kernel,
    feed_id: &str,
    scopes: &BTreeMap<String, AccountScope>,
) -> Result<Vec<String>, ImportError> {
    let mut voids = Vec::new();
    for (account_id, scope) in scopes {
        let Some(coverage) = dates::span(scope.dates.iter().map(String::as_str)) else {
            continue;
        };
        let live = kernel
            .live_observations_for_account(feed_id, account_id)
            .map_err(ImportError::kernel)?;
        for obs in live {
            if scope.present.contains(&obs.external_id) {
                continue;
            }
            if dates::covers(&coverage, &obs.occurred_date) {
                voids.push(obs.id);
            }
        }
    }
    Ok(voids)
}

fn empty_report(feed_id: &str, data_rows: u32, seq: i64) -> ImportReport {
    ImportReport {
        feed_id: feed_id.to_string(),
        data_rows,
        inserted: 0,
        revised: 0,
        unchanged: 0,
        voided: 0,
        observation_ids: Vec::new(),
        noop: true,
        seq,
    }
}

fn finish(
    feed_id: &str,
    sealed: &accounting_kernel::v2::CommitResult,
    data_rows: u32,
    imported: &accounting_kernel::v2::ImportOutcome,
    voided: usize,
) -> ImportReport {
    let unchanged = u32::try_from(imported.observation_ids.len())
        .unwrap_or(u32::MAX)
        .saturating_sub(imported.inserted)
        .saturating_sub(imported.revised);
    ImportReport {
        feed_id: feed_id.to_string(),
        data_rows,
        inserted: imported.inserted,
        revised: imported.revised,
        unchanged,
        voided: u32::try_from(voided).unwrap_or(u32::MAX),
        observation_ids: imported.observation_ids.clone(),
        noop: sealed.noop,
        seq: sealed.seq,
    }
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
        if !seen.insert((source.last4.clone(), source.external_id.clone())) {
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
    let mut unmapped = Vec::new();
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
                unmapped.push(source.line);
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
    if !unmapped.is_empty() {
        unmapped.sort_unstable();
        unmapped.dedup();
        return Err(ImportError::unmapped(unmapped));
    }
    Ok(resolved)
}
