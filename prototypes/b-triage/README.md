# Prototype B — Inbox triage

Throwaway TypeScript + HTML prototype for month 1 of your-accounting. One card per unreviewed observation, a one-click suggested treatment (recipe + category), settlement-group suggestions, a progress counter toward every line being reconciled, voided, or open, skip/defer to a worklist, and an in-browser toy ledger.

This is not the product. Sister prototypes (A, spreadsheet register; C, journal-first) live in other folders and branches — do not mix them.

## Run

From this directory (`prototypes/b-triage/`):

```bash
npm install
npm start
```

Then open http://127.0.0.1:5172/

The server binds **127.0.0.1:5172 only**. It serves the app and, by default, the synthetic fixture at `data/synthetic.csv`.

To load a real Monarch CSV at runtime (never committed, never logged):

```bash
YA_MONARCH_CSV=/absolute/path/to/export.csv npm start
```

The path is read only at process start. The server logs a row count and `YA_MONARCH_CSV` / `synthetic fixture` — never row contents, merchants, or amounts.

```bash
npm test
```

## Privacy rules

- Real data stays out of the repo, PRs, issues, CI logs, commit messages, and prompts. The only committed CSV is `data/synthetic.csv`, marked `# synthetic` on the first line.
- Load real data only through the `YA_MONARCH_CSV` environment variable. Do not copy it into this folder, into browser storage, or into a cache on disk.
- The parser never logs row contents. Parse issues report a line number and a structural reason only.
- No network egress at runtime: no CDNs, no font or script fetches, no analytics. Dependencies are installed locally and the client bundle is built on the box at startup (`public/app.js` is generated and gitignored).
- No browser storage: no `localStorage`, `sessionStorage`, IndexedDB, or cookies. Triage state lives in memory and is cleared on refresh or Reset.
- Account numbers in the fixture use a last-4 only.

## What it does

- **Inbox.** One card per observation whose Monarch `Reviewed` column is false. Pending rows are badged but still triaged here.
- **One-click treatment.** Each card offers a suggested recipe plus category: Purchase, Refund (contra-expense), Perk credit, Card payment, Own-account transfer, or Paycheck.
- **Settlement groups.** A card payment is paired with the matching bank outflow (same amount, opposite sign, date window, card vs deposit account). A paycheck is split into gross, taxes, pre-tax, Roth, HSA employee deduction, and net. An **Expected HSA** slot is shown on the card; it can stay unfilled as its own draft group (ADR 0006).
- **Progress.** Count of observed lines that are reconciled, voided, or open, over the total imported. Inbox (unreviewed) and interpreted lines are not yet there.
- **Worklist.** Skip/defer parks a line as open. Incomplete groups land as interpreted. Unfilled Expected HSA slots appear as expected movements.
- **Toy ledger.** Applying a treatment posts balanced debit/credit entries in memory.

Keyboard: `J`/`K` move, `Enter` apply, `S` skip, `V` void.

## Data shape

The fixture (and any `YA_MONARCH_CSV` file) uses the 14 Monarch columns:

`Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id`

A leading `# synthetic` comment is required on committed fixtures and is tolerated by the parser. Amounts are signed (negative = outflow). `Id` is the natural external id.

The bundled synthetic file has 37 invented October 2025 rows covering ordinary spend, card payment pairs, a paycheck with and without an HSA inflow, transfers, a refund, a perk credit, a pending charge, and some already-reviewed lines.

## Caveats (cut or stubbed)

- This is an in-browser toy ledger, not the Rust kernel. Entries are not append-only events, ids are not ULIDs, and there is no `doctor` or statement reconciliation screen (U09) or correction lineage (U10).
- Suggestions are deterministic heuristics (recipe-owned match hints), never auto-committed.
- Paycheck splits use a canned Northwind Staffing stub when the net matches the fixture; other payroll rows get a balanced estimate from net pay.
- The prototype **honors the Monarch Reviewed column**. Already-reviewed fixture rows start reconciled. A real export that marks every row reviewed will open with an empty inbox (see month-1 open question 9). Flip `Reviewed` to false in a local copy if you want those rows in the inbox; do not commit that copy.
- Already-reviewed rows get simple opening ledger entries; historical settlement groups are not reconstructed.
- Void is a confirm-free in-memory tombstone, not a kernel commit.
