# Prototype A: spreadsheet register

Throwaway TypeScript/HTML (plain ESM + JSDoc) toy ledger for Doug's month-1 feedback. One dense, keyboard-first table of October lines with in-cell recipe and category picking, filters by review state and origin, and a debit/credit preview in a side panel.

This is **not** the Rust kernel and not a product UI. Interpret / accept / void live in browser memory only. Other prototypes (B card triage, C journal-first) are separate; this folder is self-contained.

Vocabulary follows the repo glossary: observation, origin (observed / expected), review state (unreviewed / interpreted / reconciled / voided), recipe, category (leaf income or expense ledger account), posting, expected movement, contra-expense, occurred date, minor units.

## Run

Requires Node 20+.

```bash
cd prototypes/a-register && npm start
```

Then open [http://127.0.0.1:5171/](http://127.0.0.1:5171/). The server binds loopback only.

```bash
YA_MONARCH_CSV=/path/to.csv npm start
```

loads that Monarch-shaped CSV at startup instead of the committed synthetic fixture. The process reads the file once, keeps rows in memory, and serves them at `GET /api/rows`. It never writes the file, never copies it, and never logs row contents (counts and source name only).

```bash
cd prototypes/a-register && npm test
```

runs the CSV-parse and integer-cents balance tests (`node --test`).

## Keyboard

| Key | Action |
|---|---|
| `j` / `↓` | Next row |
| `k` / `↑` | Previous row |
| `Shift`+arrows | Extend selection |
| `Space` | Toggle row in the selection |
| `Home` / `End` | First / last visible row |
| `h` `l` / `←` `→` | Move between cells |
| `Enter` or `e` | Edit recipe, category, or counter-account |
| Type in a recipe/category cell | Open the combobox filtered to what you typed |
| `Esc` | Cancel edit, close help, or clear the filter |
| `/` | Focus the filter box |
| `a` | Accept / post selected lines when the preview balances |
| `x` | Void selected lines |
| `u` | Reopen a reconciled or voided line, or clear an interpreted recipe (toy; not append-only) |
| `?` | Keyboard help overlay |

The grid uses `role="grid"` / `gridcell`, visible focus, and works without a mouse.

## Data rules

- Default data is `data/synthetic-october.csv`: invented October 2026 Monarch-shaped rows, first line `# synthetic`, then the 14 columns `Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id`. Merchants and account last-4s are obviously fake.
- The CSV parser skips a leading `#` comment line and handles quoted fields that contain commas, quotes, or newlines.
- Every committed CSV row is an **observed** feed row. A few **expected** movements (HSA counterparts for the synthetic paychecks, plus a recurring rent and insurance draft) are added in memory for the synthetic fixture only. They are not invented on top of `YA_MONARCH_CSV`.
- Monarch `Reviewed` and `Pending` are not review-state truth. Every loaded line starts **unreviewed**. Pending is shown as a badge only.
- Amounts are integer cents (`amount_minor`). No floats in parse, preview, or totals. Debits equal credits when a recipe preview is complete.
- Privacy guarantees:
  - No real rows in this folder or in git.
  - Team bots must not read `/workspace/ya-private-data`.
  - Server logs counts only, never row contents or the env path.
  - No network egress from the page (no CDN scripts or fonts; Content-Security-Policy `default-src 'self'`).
  - No browser storage: no `localStorage`, `sessionStorage`, IndexedDB, or cookies. State is in memory and is gone on refresh.

## Known gaps

- In-memory only; a refresh loses interpretation, posting, and voids.
- Accept/post is a state flip, not an append-only journal commit (no `recorded_at`, reversals, or `superseded_by`).
- No settlement grouping across lines (paycheck and its expected HSA stay separate rows; the paycheck preview is a canned gross-to-net sketch).
- No statement / occurred-date reconciliation screen, and no correction lineage / audit toggle.
- Expected movements ship only with the synthetic fixture.
- Category list is a small invented seed, not Doug's real chart of accounts. Entity tags (`e-*`) are hints only; books are not isolated per entity.
- USD only. Posted date is absent from the Monarch shape, so in-flight items are not modeled here.
- The side-panel preview is the only journal view; there is no packed ID, export, or persistence API.
