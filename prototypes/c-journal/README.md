# Prototype C — journal first

Throwaway accountant view. It is not the month-1 app. Other prototypes live in their own folders.

## Run

From the repo root:

```bash
node --experimental-strip-types prototypes/c-journal/server.ts
```

Serves `http://127.0.0.1:5173` and loads the synthetic Monarch CSV in `fixtures/`. Corrections stay in memory. Reload discards them. The page does not use localStorage, IndexedDB, or cookies, and it does not call any other host.

## Real CSV

```bash
YA_MONARCH_CSV=/path/to/file.csv node --experimental-strip-types prototypes/c-journal/server.ts
```

The file is read when the page asks for it. Rows are not logged, copied into the repo, or written to browser storage. `fixtures/synthetic-books.json` is not applied: there are no opening balances and no posted dates, so the in-flight lists stay empty and the difference is the typed statement ending minus the occurred balance.

The CSV must use the 14 Monarch columns: Date, Merchant, Category, Account, Original Statement, Notes, Amount, Tags, Owner, Reviewed, Reviewed At, Reviewed By, Pending, Id. Dates may be `YYYY-MM-DD` or `M/D/YYYY`.

## What the screen shows

- **Journal** and **T-accounts.** Debits and credits are explicit. Categories are income and expense accounts. A card purchase is a debit to a leaf expense and a credit to the card.
- **Reconciliation.** Occurred balance, in-flight items in both directions, a statement ending you type, and the difference. On the synthetic Visa •4410 at 2025-10-31 the invented ending is 2,075.44, and the in-flight items explain it exactly until you correct something.
- **Corrections.** Edit appends a reversal and a replacement. The journal hides that pair until Audit is on. **Correction of a correction** walks Harbor Office Supply from the imported 950.00 to 90.00 and then to 95.00. The feed row stays 950.00. Audit lists all five events.

`node --experimental-strip-types --test prototypes/c-journal/test/ledger.test.ts` checks the fixture math.

## Assumptions

Where the spec and the later notes disagree, the prototype follows the 2026-11-22 decisions: pure double entry (ADR 0008), append-only reversal plus `superseded_by` (ADR 0002 amendment), and occurred-date reconciliation. The typed statement ending is not rewritten when a correction changes the balance, and this toy does not reject a correction that would move a closed month. Posted dates exist only on the synthetic fixture. Cross-entity rows post both legs in this one in-memory ledger. Amounts are integer cents. The feed is treated as USD. An env file's card-versus-cash guess is by account name, unknown `e-*` tags are one-sided claims, and a paycheck here is net wages only. Reviewed and Pending are not treated as truth.
