# Contributing

## Privacy guards

Real financial data stays out of git. Run every guard locally before opening a pull request:

```bash
bash scripts/check-privacy.sh
```

The `privacy` job in [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs that script and then `bash scripts/test-privacy.sh` on every pull request and on every push to `main`, with `YA_DENYLIST` taken from the repository secret (it may be empty). If a script is not in the tree yet, that step prints a notice and is skipped.

`bash scripts/test-privacy.sh` plants a synthetic violation for each guard and checks that the guard fails, then runs the same guard against the clean tree. `YA_DENYLIST` may be empty. Acme Corp is the employer placeholder in fixtures, docs, and tests.

| Guard | Command | What it rejects |
| --- | --- | --- |
| Data files | `bash scripts/check-privacy.sh data-files` | CSV, QFX, OFX, PDF, DB, and SQLite tracked outside a `fixtures/` directory at any depth (`fixtures/`, `prototypes/name/fixtures/`). JSON counts only under a `fixtures/` directory, or when it carries a synthetic-data marker. `package.json`, `package-lock.json`, and `tsconfig.json` are not data files. |
| `private/` | `bash scripts/check-privacy.sh private` | A `private/` path that is not gitignored, or any tracked file under `private/`. Real statements and `private/denylist.txt` live there at runtime. |
| Denylist | `bash scripts/check-privacy.sh denylist` | Identifying strings in tracked text. Also `bash scripts/check-denylist.sh`. |
| Account numbers | `bash scripts/check-privacy.sh account-numbers` | Account-number-like digit runs in tracked text data, markdown, `docs/`, and `prototypes/`. Masked last-4 forms such as `****1234` are allowed. A JSON value of at most six digits on a key ending in `minor` is a minor-unit amount. |
| Logs | `bash scripts/check-privacy.sh logs` | A log line from transaction processing that still contains the payee, description, or memo. Skips with exit 0 when `accounting-privacy` or `cargo` is absent. |

The matching planted-violation test is the same guard name passed to `bash scripts/test-privacy.sh`. The log planted case is the `planted_unredacted_log_fails_the_guard` test in `accounting-privacy`.

### Denylist terms

The committed placeholder file `scripts/denylist.placeholders` holds synthetic terms used to prove the guard fails. It is not a list of real names. Real terms are not stored in the repository. Put them in either place:

- `private/denylist.txt`, one term per line. `private/` is gitignored.
- `YA_DENYLIST`, one term per line (a CI secret). `YA_DENYLIST_FILE` may point at a file outside the repo instead.

Matching is case-insensitive. The check prints locations only, never the term.

`scripts/denylist.sha256` still lists SHA-256 digests of lowercase word tokens from before terms moved out of the repo. The check fails if a tracked token hashes to one of those digests. To record another digest without committing the word:

```bash
printf '%s' "$word" | sha256sum
```

Append only the hex digest. Do not commit the word.

### Data files

The placement guard looks at CSV, QFX, OFX, PDF, DB, SQLite, and SQLite3. Those files must live under a directory named `fixtures` (top-level `fixtures/` or a per-prototype directory such as `prototypes/name/fixtures/`). Text files there must contain `# synthetic` or `<!-- synthetic -->`. PDF may use a `% synthetic` comment. DB and SQLite may instead have a tracked sibling `<file>.synthetic` whose contents include `# synthetic`.

JSON is a data file only when it is already under a `fixtures/` directory, or when it carries a synthetic-data marker. A marker is any of the text markers above, a top-level `"_synthetic": true` boolean (a nested `_synthetic` key does not count), or a tracked sibling `<file>.json.synthetic` that contains `# synthetic`. `package.json`, `package-lock.json`, and `tsconfig.json` have none of those, so they pass. A marked JSON file outside every `fixtures/` directory fails, because it is synthetic data in the wrong place.

### Account numbers

The check reads every tracked text data file (CSV, TSV, JSON, OFX, QFX, and the other text statement extensions), every markdown file, every file under a `docs/` directory, and every text file under `prototypes/`.

Inside a `fixtures/` directory, any run of more than four consecutive digits fails. `****1234`, `...1234`, and `xxxx1234` pass because only four digits are visible.

Outside fixtures, the same rule applies, with two allowances so ledger examples stay valid. A run of at most six digits on a line that names a currency, minor units, debit, or credit is treated as an amount, not an account number. A token that contains a letter (a commit id, a ULID, a hash) is not an account number. An IBAN-shaped token and a hyphenated group of three or more four-digit blocks still fail. Dates (`2026-11-27`) and two-group ranges (`0002-0004`) do not.

A JSON value of at most six digits is also an amount when its key ends in `minor`, in any casing. `openingMinor`, `fromAmountMinor`, and `amount_minor` are the usual names. The exemption is the value of that key only, and it applies inside fixtures as well. A longer run on a `minor` key still fails, and a key that merely contains the letters, such as `minorAccount`, does not qualify.

### Logs

`accounting-privacy` is the redacting logger. `process_transaction` writes the id and replaces payee, description, and memo with `[redacted]`. The crate tests process a synthetic Acme Corp transaction and reject a planted log line that keeps that text.

If `cargo` is not on `PATH`, or `accounting-core/crates/accounting-privacy/Cargo.toml` is missing, the guard prints `logs: SKIP` and exits 0. When the crate is present, a failing or missing redaction test is still a failure.

## Data rules

Real financial data must never be committed, opened in a PR, or printed in
CI logs. The app reads Doug's local files only at runtime, through
`YA_MONARCH_CSV` or a gitignored `local.config.json`. Committed CSV and JSON
must be generated fixtures.

A [data guard](scripts/data_guard.py) also runs as the `data-guard` CI job and
as an optional pre-commit hook. It inspects tracked (CI) or staged (hook)
files and fails if:

- a CSV or JSON file is missing the synthetic marker
- a CSV file sits outside a `fixtures/` directory
- a line contains an account-number-like digit run longer than 4 digits
  (last-4 values are allowed)

Findings report file and line only; the matched text is never printed. This
complements `scripts/check-privacy.sh`; both must pass.

### Synthetic marker

**CSV.** Put a comment line whose first token after `#` is `synthetic`,
normally on the first line:

```csv
# synthetic
Date,Merchant,Amount
2025-10-01,Example Shop,1.00
```

The U14 placement guard also accepts `<!-- synthetic -->`.

**JSON.** Prefer a top-level `"_synthetic": true` key so the file stays
valid JSON:

```json
{
  "_synthetic": true,
  "rows": []
}
```

JSON arrays cannot carry that key, so they (or any JSONC file) may start
with a leading comment instead:

```jsonc
// # synthetic
[]
```

The committed October fixture at `fixtures/monarch/october-2025-synthetic.csv`
already follows this convention (first line `# synthetic`).

### Pre-commit hook

```bash
git config core.hooksPath .githooks
```

The hook runs `python3 scripts/data_guard.py` on staged files. CI runs the
same script with `--all` over every tracked file, plus the guard's unit
tests.

### `.gitignore`

`local.config.json`, `local/`, `data/`, `ya-private-data/`, `private-data/`,
`private/`, `*.sqlite`, `*.db` and SQLite sidecar files are ignored. Keep
real extracts outside the working tree (for example
`/workspace/ya-private-data/`). Real denylist terms stay in
`private/denylist.txt` or in `YA_DENYLIST`. Never commit them.

## CI

The GitHub Actions workflow in [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
runs on pull requests and on pushes to `main`. It has five jobs:

- **rust** operates in `accounting-core/` (the Cargo workspace) and runs
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` on the stable toolchain.
- **smoke** runs `docs/schema-v2-smoke.sh` (synthetic data only).
- **denylist** runs `scripts/check-denylist.sh`.
- **data-guard** runs `scripts/data_guard.py --all` and the guard unit tests.
- **privacy** runs `scripts/check-privacy.sh` and then
  `scripts/test-privacy.sh`, with `YA_DENYLIST` taken from the
  `YA_DENYLIST` repository secret (it may be empty). If a script is
  not in the tree yet, that step prints a notice and is skipped.
