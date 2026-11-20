# ADR 0005: Accounts have a tracking mode; investment accounts use valuation snapshots

Status: Proposed (revised 2026-11-20)

## Context
Some destinations (HSA, 401k, brokerage) can be tracked in full or treated as a sink.

## Decision
- A financial account is **transparent** (balance tracked; money in and out are transfers) or **black box** (money sent there is booked expense-like, e.g. "investment contribution"). Both are supported; the mode is chosen at account creation.
- Retirement deferrals (pre-tax 401k, Roth 401k, HSA) are transfers into transparent accounts of `investment` kind. Month 1: HSA is transparent.
- Investment accounts are owned by an entity, not entities themselves.
- **No gain/loss posting.** Market value is recorded as point-in-time **valuation snapshots** when available and compared against book basis (contributions + dividends - withdrawals). Unrealized gain/loss is computed for display, never journaled; only realized events are transactions. Valuation snapshots are allowed only on `investment` accounts, so cash/card balance checks (INV-011) stay meaningful.
- Switching modes later is a wizard that only appends commits (ADR 0002). **Deferred past month 1.**

## Consequences
Removes the risk of a plug absorbing unresolved expected items. Pre-tax vs Roth sub-accounts, holdings detail and taxable brokerage dividends are deferred.
