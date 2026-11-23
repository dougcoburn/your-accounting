# ADR 0008: Kernel is pure double-entry; categories are income/expense ledger accounts

Status: Accepted (2026-11-22, Doug)

## Context
Spec v0.2 split the books in two: balance-sheet lines lived in `journal_postings`, while income and expense lived in a separate `allocations` table keyed to a `categories` tree. A purchase therefore never appeared in the journal as "debit expense, credit card liability", and the balance (Assets = Liabilities + Equity + Income - Expenses) could not be proved from postings alone. Doug expected ordinary double-entry in the kernel.

## Decision
- Every ledger account has exactly one **account type**: asset, liability, equity, income, or expense (`ledger_accounts.account_type`, CHECK-constrained). Engine sub-kinds (cash, card, receivable, payable, clearing, opening equity, category) stay in `kind`.
- **Categories are income/expense ledger accounts.** They are per entity, hierarchical through `parent_id` (same entity, same type), and only leaf accounts receive postings. The `categories` table is removed.
- **Income and expense amounts are ordinary journal postings.** The `allocations` table is removed. A card purchase is a debit to a leaf expense account and a credit to the card's liability account.
- **Contra amounts** (refunds, rebates, card-perk statement credits, discounts) are opposite-side postings to the original income/expense account, so they reduce that category and are never income (GAAP; MAP decision 2026-11-19). No separate contra type.
- Tags attach to postings (`posting_tags`, replacing `allocation_tags`); `refund_links` link a refund posting to purchase posting(s).
- The leaf/same-entity rule moves from the allocation trigger to postings: a posting to an income/expense account must use a leaf account of the posting's entity.
- **Kernel invariant (INV-001, strengthened):** for every accounting event, sum(debits) = sum(credits) per (entity, currency) within `journal_postings` alone. The trial balance and A = L + Eq + (I - X) close from that one table.
- Unchanged: settlement exclusivity on posting lines (ADR 0006), per-entity databases (ADR 0004), and valuation snapshots outside the journal on non-cash assets only (ADR 0005).

## Consequences
- One table answers both "what happened to the bank/card" and "what did we spend on groceries"; P&L and balance sheet are views over postings.
- Category edits are ledger-account edits. Moving history between categories is a correction (reversal plus repost, ADR 0002), not a relabel. Re-parenting a category whose leaf has postings needs a migration rule (deferred).
- Schema delta against v0.2: drop `categories`, `allocations`, `allocation_tags`, `allocation_category_valid`; add `account_type`, `parent_id`, `posting_tags`, `posting_category_leaf`; repoint `refund_links`.

## Alternatives considered
- **Keep the v0.2 allocations model** (balance-sheet postings plus a separate P&L allocations table). Rejected: two sources of truth, and the balance cannot be proved from postings alone, so the kernel's central invariant would depend on cross-table reconciliation.
- **Categories as tags or metadata on postings to one generic expense account.** Rejected: category totals would not be ledger balances, and leaf/ownership rules would live outside the kernel.
