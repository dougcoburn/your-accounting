# synthetic October 2025 Monarch fixture

Invented Monarch Money export for October 2025. Merchants, notes, and ids are fictional. Account names show at most a last-4.

`monarch-synth` writes `october-2025-synthetic.csv` beside this file. From `accounting-core`:

```
cargo run -p monarch-synth -- --check
```

`--check` confirms the committed CSV and this file match the default seed (`DEFAULT_SEED`, the digits of 2025-10-15). `--seed` changes filler merchants, amounts, and days. Scenario rows, and the ids in the table below, stay put.

Every `Date` is in October 2025 (2025-10-01 through 2025-10-31). Amounts are signed: negative is an outflow, positive is an inflow, and none are zero. Every row has `Pending` false, `Reviewed` true, a `Reviewed At` timestamp, and a `Reviewed By` value.

## Columns

`Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags,Owner,Reviewed,Reviewed At,Reviewed By,Pending,Id`

## Shape

- Data rows: 200
- Distinct accounts: 12
- Distinct categories: 44
- Negative amounts: 80 percent
- Positive amounts: 20 percent
- Transfer or payment categories: 15 percent
- Distinct e-tags: 7
- Untagged rows: 2
- Rows with notes: 6
- Owner: Casey Sample

## Accounts

- Bramble Brokerage ...3160
- Cinder Mastercard ...5590
- Cinder Visa ...5582
- Dragonfly Operating ...9936
- Fable Rewards ...6603
- Harbor Joint Checking ...3340
- Lumen Cash ...4471
- Nimbus Store Card ...7714
- Oak Mortgage ...1048
- Pebble Checking ...2210
- Pebble Savings ...2218
- Sable Auto Loan ...2059

## Tags

- e-acme
- e-dragonfly
- e-household
- e-personal
- e-sample
- e-sinking
- e-taxset

## Scenario coverage

The ids below are the rows that exercise each scenario. Filler rows use the same columns and are not listed here.

| Scenario | Ids | What the rows show |
| --- | --- | --- |
| Card buys | s-card-01, s-card-02, s-card-03 | Negative purchases on fictional credit cards. |
| Credit-card payment (both sides) | s-cc-out, s-cc-in | Checking outflow and card inflow on the same date with opposite signs, both Credit Card Payment. |
| Paycheck | s-pay-01, s-pay-02 | Positive Paychecks rows from Acme Corp Payroll (fictional), tagged e-acme. |
| HSA contribution (matching side missing) | s-hsa-01 | Checking outflow in HSA Contribution. The HSA deposit side is absent from this file. |
| Dragonfly e-tagged buy | s-dfly-01 | Negative e-dragonfly purchase for Dragonfly Homes on a household credit card. |
| Acme Corp partial reimbursement | s-exp-01, s-reimb-01 | e-acme expense paired with a smaller e-acme Reimbursements inflow. The note says the reimbursement is partial and the remainder stays open. |
| Refund | s-refund-01 | Positive Groceries amount on a card, original statement contains RETURN, merchant matches an earlier buy. |
| Card perk or statement credit | s-perk-01 | Positive Subscriptions amount on a card whose original statement contains STATEMENT CREDIT. |
| Duplicate row | s-dup-a, s-dup-b | These two rows share every field except Id. |
| In-flight item | s-fly-01 | Late-October transfer outflow. The note says the other side posts in November and that side is absent from this file. |
