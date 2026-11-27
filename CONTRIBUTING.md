# Contributing

## Privacy guards

Real financial data stays out of git. Run every guard locally before opening a pull request:

```bash
bash scripts/check-privacy.sh
```

That is the single command CI should call. There is no GitHub Actions workflow in this repository yet (U01). When one is added, include:

```yaml
- name: Privacy guards
  run: bash scripts/check-privacy.sh
  env:
    YA_DENYLIST: ${{ secrets.YA_DENYLIST }}
- name: Privacy guard self-test
  run: bash scripts/test-privacy.sh
```

`bash scripts/test-privacy.sh` plants a synthetic violation for each guard and checks that the guard fails, then runs the same guard against the clean tree. `YA_DENYLIST` may be empty. Acme Corp is the employer placeholder in fixtures, docs, and tests.

| Guard | Command | What it rejects |
| --- | --- | --- |
| Data files | `bash scripts/check-privacy.sh data-files` | PDF, CSV, TSV, JSON, QFX, OFX, QBO, QIF, OFC, IIF, XLS, XLSX, DB, and SQLite tracked outside `fixtures/`. Inside `fixtures/`, text files must contain `# synthetic` or `<!-- synthetic -->`. PDF may use a `% synthetic` comment. Binary files may instead have a tracked sibling `<file>.synthetic` whose contents include `# synthetic`. |
| `private/` | `bash scripts/check-privacy.sh private` | A `private/` path that is not gitignored, or any tracked file under `private/`. Real statements and `private/denylist.txt` live there at runtime. |
| Denylist | `bash scripts/check-privacy.sh denylist` | Identifying strings in tracked text. Also `bash scripts/check-denylist.sh`. |
| Account numbers | `bash scripts/check-privacy.sh account-numbers` | Account-number-like digit runs in `fixtures/`, `docs/`, and `accounting-core/docs/`. Masked last-4 forms such as `****1234` are allowed. |
| Logs | `bash scripts/check-privacy.sh logs` | A log line from transaction processing that still contains the payee, description, or memo. |

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

### Account numbers

In fixtures, any run of more than four consecutive digits fails. `****1234`, `...1234`, and `xxxx1234` pass because only four digits are visible.

In docs, the same rule applies, with two allowances so ledger examples stay valid. A run of at most six digits on a line that names a currency, minor units, debit, or credit is treated as an amount, not an account number. A token that contains a letter (a commit id, a ULID, a hash) is not an account number. An IBAN-shaped token and a group such as `1234-5678-9012` still fail. Dates (`2026-11-27`) and two-group ranges (`0002-0004`) do not.

### Logs

`accounting-privacy` is the redacting logger. `process_transaction` writes the id and replaces payee, description, and memo with `[redacted]`. The crate tests process a synthetic Acme Corp transaction and reject a planted log line that keeps that text.
