# Glossary

## Language

**Observation**:
One imported record of money moving in one account, as reported by a feed. For month 1 a Monarch CSV row. Its imported category is a suggestion, not truth.
_Avoid_: transaction (ambiguous with journal entry)

**Feed**:
A source of Observations (month 1: Monarch CSV export; manual offline extraction for accounts Monarch lacks).

**Evidence**:
A document supporting an Observation or interpretation (later: saved bank statements). Not a feed in month 1.

**Accounting entity** (user-facing synonym: Division):
A set of books, stored in its own database. Month 1: Doug, Household, Dragonfly (business). Esther and Acme Corp are Parties. See ADR 0003.

**Visibility** (viewer-relative):
Determined by a user's access grants, not by the entity. An entity a viewer cannot access appears to them as a Party. See ADR 0003. _Avoid_: "opaque entity".

**Entity tag** (e-*):
Monarch tag Doug uses to fake entities; import maps e-* tags to Accounting entities.

**Reimbursement claim**:
A receivable (AR) created when an expense is paid on behalf of another entity; settled only by a linked reimbursement, or closed by a Write-off.

**Write-off**:
Closing an unpaid Reimbursement claim by reclassifying it (e.g. as a gift).

**Expected movement**:
A movement a recipe says must exist in another account (e.g. HSA receipt for a payroll deduction) that has not been observed. Shown as an unreviewed (draft) register item in its own uncommitted group until its counterpart exists; never parked in committed clearing (ADR 0006).

**Black-box account** (candidate name):
A destination treated like an expense (e.g. "investment contribution") for money leaving tracked books into an account not modeled in detail.

**Review state** (per line item):
Unreviewed (no recipe chosen), Interpreted (draft: recipe chosen, group not yet complete), Reconciled (settlement group complete and committed), Voided. Agreement with a statement is a separate term: Statement-matched. A transaction's overall state is derived from its lines.

**Origin**:
Observed (from a feed row) or Expected (a recipe says it should exist, not yet seen).

**Tracking mode** (of a financial account):
Transparent (balance tracked; money in and out is transfers) or Black box (money sent there is booked like an expense; nothing inside is tracked). Changed only through the mode-switch wizard.

**Ledger account**:
An account in one entity's books that postings hit. Has exactly one Account type. Financial accounts (checking, card) map to asset/liability ledger accounts; Categories are income/expense ledger accounts (ADR 0008).

**Account type**:
One of asset, liability, equity, income, expense. Fixes which side increases the account (debit for asset/expense, credit for the rest). Contra amounts are opposite-side postings, not a sixth type.

**Category**:
An income or expense Ledger account, per entity, arranged in a tree; only leaf categories take postings (ADR 0008).

**Posting**:
One debit or credit line in the journal: ledger account, currency, side, positive `amount_minor`. The entity is the database, not a column (ADR 0004). Every event's postings balance per currency, so the books balance from postings alone.

**Allocation** (retired 2026-11-22):
Spec v0.2's separate P&L record. Replaced by ordinary postings to Category accounts (ADR 0008). _Avoid_: "allocation"; say posting or category posting.

**Contra-expense** (refund, rebate, statement credit):
A credit posting to the original expense category that reduces it rather than counting as income. _Avoid_: "income" or "reward" for card-perk credits (implies taxability).

**Income**:
Reserved for inflows that are genuinely earned or received (wages, interest, business revenue). Not refunds or reimbursements.

**Match hint**:
A rule a recipe declares to find candidate participants for its settlement group (amount, date window, party, memo pattern, max participants). Hints live on the recipe, not in a global matcher.

**Commit** (correction commit):
An atomic, append-only batch of journal entries (reversals plus replacements). Valid only if every transaction and settlement group it touches is valid afterward. _Avoid_: "edit" for anything in the journal.

**Append-only journal**:
Journal entries are never updated or deleted; apparent edits in the UI are reversals plus new entries. The one exception is the Superseded flag (ADR 0002 amendment).

**Superseded** (`superseded_by`):
A flag on a journal entry pointing to the entry that reverses it, set only in that reversal's commit. Hides the pair from the register; balances ignore it; `doctor` can rebuild it from reversal links. _Avoid_: "deleted" or "edited".

**Effective date vs. recorded-at**:
Effective date is when the entry counts for reporting (for corrections, the observation's date). Recorded-at is when the entry was appended to the journal.

**Minor units** (`amount_minor`):
Money stored as an integer count of the currency's smallest unit (cents for USD), always paired with a currency code. _Avoid_: floats, "dollars" as a stored value.

**Non-taxable fringe benefit** (compensation category):
Employer-paid value received but excluded from taxable wages (e.g. employer health premiums, IRC §106; employer HSA contributions, §223/W-2 box 12 W). Grossed up as compensation plus an equal expense or transfer. _Avoid_: plain "income" (implies taxable).

**Investment income** (dividends, interest, distributions):
A real event inside an investment account, journaled as a transaction with its own `recorded_at`. Part of book basis. _Avoid_: lumping it into gain/loss.

**Book basis** (non-cash asset account):
The account's balance from the journal alone. Investments: contributions + investment income - withdrawals. Real estate and other fixed assets: purchase cost + improvements (ADR 0005).

**Valuation snapshot**:
A point-in-time estimate of a non-cash asset account's market value (a brokerage statement, an appraisal or home-value estimate), recorded outside the journal (never a posting). Allowed on non-cash asset accounts such as investments, real estate and vehicles; never on cash, card or liability accounts.

**FX translation** (deferred):
Restating a foreign-currency account's exact balance in the reporting currency at an exchange rate on a date. Exchange rates are stored per currency pair per date; the translated value and any translation gain/loss are display-only, never journaled. Not a valuation snapshot: the account's balance check stays in its own currency.
ADR 0009 (Doug, 2026-11-26) adds no translation: there is no month-end revaluation and no single reporting currency, and each currency is reported on its own. This entry stays deferred and display-only.

**Account currency** (ADR 0009, post-month-1):
The one currency of a ledger account, fixed when the account is created. Every posting on the account must be in that currency, or the kernel rejects it (`CurrencyMismatch`). Only the FX Clearing account and logical category nodes have none. A ledger account backed by a financial account has that account's currency. _Avoid_: "the account's main currency" (there is only one).

**Logical category vs. category ledger account** (ADR 0009, post-month-1):
A logical category (Dining, Bank fees) is currency-independent and takes no postings. The ledger account that takes its postings is unique per (category, currency), such as Dining (USD) and Dining (EUR). A category is never reported across currencies; each currency's P&L rolls up the logical tree on its own. _Example_ (synthetic): a USD card dinner in Germany posts to Dining (USD); the entity needs no Dining (EUR) unless it pays EUR expenses from EUR accounts.

**FX Clearing account** (ADR 0009, post-month-1):
The one multi-currency ledger account, one per entity (kind `fx_clearing`). An exchange passes through it: the currency given up is debited to it, and the currency received is credited from it, in the same event. It never shows a balance; reports show it as a list of closed FX groups with their rates. Not the ADR 0006 `clearing` kind used by settlement groups. _Avoid_: "FX suspense", "FX gain/loss account".

**FX group** (exchange transaction; ADR 0009):
One accounting event that has FX Clearing rows. Its FX Clearing debit rows are all in one currency and its credit rows all in one different currency, each side with at least one row; the kernel rejects any other shape, including the same currency on both sides. It is closed when it commits. _Example_ (synthetic): Tx 15 debits FX Clearing EUR 1,000.00 and credits it USD 1,080.00 and USD 105.00.

**Derived rate** (ADR 0009):
An FX group's FX Clearing credit total divided by its debit total, in units of the credit currency per unit of the debit currency, after adjusting for each currency's minor-unit exponent: (credit minor / debit minor) x 10^(debit exponent - credit exponent). Derived from the posted amounts, never entered and never stored as authoritative; a cache, if ever added, is non-authoritative. Shown rounded to 7 decimal places, half-even. _Example_ (synthetic): USD 1,185.00 / EUR 1,000.00 = 1.1850000 USD per EUR; its reversal is 0.8438819 EUR per USD; USD 100.00 against JPY 15,000 is 150.0000000 JPY per USD, not the raw minor-unit ratio 1.5.

**Effective balance** (of FX Clearing; ADR 0009):
FX Clearing's balance with each FX group's debit total converted at that group's own derived rate. It is zero after every event by construction, because the kernel only accepts groups with a defined rate. Raw per-currency sums of FX Clearing are never reported as a balance. _Avoid_: "FX Clearing balance" for the raw per-currency sums.

**Net exchanged** (ADR 0009):
Per currency and period, the net amount of that currency that went out or came in through FX Clearing, shown as a flow line backed by the list of closed FX groups. A presentation in the isolated per-currency month-end reports, never an account balance and never a gain or loss.

**Realized exchange gain/loss** (ADR 0009; later report):
The result of several exchanges between the same currency pair over time, such as a USD to EUR to USD round trip. _Example_ (synthetic): USD 1,185.00 became EUR 1,000.00, which later came back as USD 1,150.00, a realized loss of USD 35.00. A later report computed by the application, never a kernel posting. _Avoid_: confusing it with net exchanged.

**Rate plausibility check** (ADR 0009):
An application-level check, outside the kernel `doctor`, that asks a rate-source port about each current, non-reversal FX group's pair and effective date. The port is an interface expected to stay in flux; one implementation is a person's gut-check review. An implausible rate becomes a warning-level to-do (look for a missing tranche or fee, or call the bank). Information only: it never writes, never blocks a commit, and always exits 0. It matters because the zero effective balance is true by construction and cannot catch a wrong amount.

**Unrealized gain/loss** (market move):
Valuation snapshot minus book basis. Derived for display only, never journaled. Only a realized event (e.g. a sale) becomes a transaction. _Avoid_: "valuation adjustment" or a plug entry.

**Transaction date vs. posting date**:
Transaction date (the Occurred date) is when the purchase happened. It is the effective date and alone decides the period for everything, including reconciliation. Posting date (the Posted date) is when the institution posted it; it is kept only to explain In-flight items against a statement, never to reconcile (2026-11-22; supersedes the 2026-11-19 rule that reconciled on posting date). _Avoid_: "reconcile by posting date".

**Party**:
Anyone outside the transparent books: merchants, employers, people (e.g. Esther, Acme Corp, Costco). Can hold obligations. UI may label as vendor/person/organization. _Avoid_: "opaque entity" as a separate concept; an entity a viewer cannot see simply appears as a party to that viewer.

**Party id**:
The ULID of a `parties` row: the identity of a counterparty (a merchant, a person, an employer, or another entity) inside one entity database. Postings and obligations name counterparties by party id.

An entity also has a party id of its own, `entity.party_id`, fixed when the entity is created. It is the **same value in every database** where that entity appears as a party: the party row that stands for that entity in another database has `parties.id = entity.party_id` (ADR 0003).

_Example_: Dragonfly's `entity.party_id` is `01J...PD` (invented). In Household's database, the party row "Dragonfly" has id `01J...PD`, and Household's AR / Dragonfly posting has `party_id = 01J...PD`. Acme Corp, which is not an entity, has a party id only in the databases that know it. _Avoid_: using a party id to locate a database; that is the Registry's job.

**Entity id**:
The ULID `entity.id` of the singleton `entity` row in an entity database. It identifies that set of books. Each database has exactly one, and rows carry no `entity_id` column, because the database *is* the entity (ADR 0004).

It is a different value from the same entity's `entity.party_id`. The entity id answers "which books is this?", and the party id answers "who is this counterparty?".

_Example_: Household's database has `entity.id = 01J...EH` and `entity.party_id = 01J...PH`. Dragonfly's database never stores `01J...EH` as a party; it stores it only as a counterpart entity id on recipe halves.

**Counterpart entity id** (`counterpart_entity_id`):
On one half of a cross-entity recipe, the entity id (`entity.id`) of the *other* entity. It is stored on each half's `accounting_events` row, together with `recipe_instance_id`; the two are both set or both NULL. It is never this database's own `entity.id`.

_Example_: Household's half of "Household pays Dragonfly's $1,200.00 insurance" has `counterpart_entity_id = 01J...ED` (Dragonfly's `entity.id`), with one counterpart expectation, USD 120000. Dragonfly's half has `counterpart_entity_id = 01J...EH`.

**Recipe id** (two meanings; say which):
- **Recipe code**, column `accounting_events.recipe_id`: the stable code of the recipe *type* that built an event, with `recipe_version`. Not a ULID. _Example_: `card_purchase` version `1.0.0` (AT-012).
- **Recipe-instance id**, column `accounting_events.recipe_instance_id`: a ULID minted once per cross-entity recipe *occurrence*. It is shared by its two halves, one in each entity database. This is the "recipe id" of ADR 0004's cross-entity model. One current event per database may carry a given instance id; an identical retry is a no-op, and a different payload is a conflict. A reversal does not carry it; a corrected replacement keeps it.

_Example_: the insurance payment is instance `01J...R1` on both halves. Dragonfly's later repayment is a new instance `01J...R2`. Both halves of each use the same recipe code.

**Counterpart expectation** (`event_counterpart_expectations`):
One row per currency on a recipe half: the amount (`expected_amount_minor` > 0) that the *other* half must carry in that currency on its intercompany account for this entity. A recipe half has at least one row and at most one per currency. A reversal and a plain event have none. The rows are part of the retry payload, so changing one is a different write.

_Example_: Household's half of a payment that also covers a fee in euros has two rows, USD 120000 and EUR 2500. Dragonfly's half must then show 1,200.00 USD and 25.00 EUR on AP / Household.

**Intercompany account; half amount**:
On a recipe half, the intercompany postings for the counterpart are the postings on a `receivable` or `payable` ledger account whose `party_id` is the counterpart's `entity.party_id`. A half's amount in a currency is the absolute net (debits minus credits) of those postings in that currency. Other postings on the same event, such as cash, an expense, an FX difference, or a fee account, do not count. The recipe check compares each half's amount per currency with the other half's expectation for that currency, and expects the two nets on opposite sides.

_Example_: Household's half is Dr AR / Dragonfly 1,200.00, Cr checking 1,200.00. Its intercompany amount is USD 1,200.00 (net debit). Dragonfly's half, Dr insurance expense, Cr AP / Household 1,200.00, has USD 1,200.00 (net credit). They agree.

**Registry**:
The application's own database (layer 3), one per user login. It maps each entity the user may see, by entity id and party id, to the file location of that entity's database. Entities enter it only by explicit registration; there is no discovery. Before any write, the application opens the target database and checks that its `entity` row (`id`, `party_id`) matches the registry row, and it refuses on a mismatch (`RegistryMismatch`). Different logins can have different, overlapping registries.

_Example_: Doug's registry lists Household (`01J...EH`, `01J...PH`, `household.sqlite`) and Dragonfly (`01J...ED`, `01J...PD`, `dragonfly.sqlite`). Another user's registry lists only Dragonfly, so that user's recipe check reports Dragonfly's halves as "counterpart not visible", never as a missing half.

**How these relate**: a cross-entity recipe instance (`recipe_instance_id`) has one half in each entity database. Each half names the other entity by its entity id (`counterpart_entity_id`) and names the other entity as a counterparty on its postings by party id (`entity.party_id` of the other entity). The registry turns either id into a database file, and the application checks that file's `entity` row before writing.

**Health check**:
Two levels.
- The per-database `doctor` reads one entity database, read-only, and checks that database's own invariants (balance per currency, sealed commits, integrity, evidence hashes).
- The application-level **recipe check** reads every entity database in the user's Registry and checks cross-entity recipes:
  - **counterpart not visible:** the counterpart's database isn't registered or can't be opened by this login. This is information, not a failure.
  - **missing half:** the counterpart database is visible and lacks the half.
  - **orphaned for 7 days:** a missing half older than 7 days is a warning, and the app offers to reverse it. A redo uses a new recipe-instance id.
  - **duplicate**
  - **mismatched counterpart**
  - **mismatched amount**, per currency, on the intercompany account

Both run at least on open. Neither repairs anything; findings surface as flagged open items. See ADR 0004.

**Void** (of an observation):
Removing an erroneous or duplicate observation from the books. Appears as a delete to the user; recorded as an append-only tombstone commit. Releases anything that observation was linked to.

**Worklist**:
All observations not yet reconciled or voided. Open is a to-do state, not an end state.

**Occurred date**:
The date a transaction actually happened. It alone decides which period a transaction belongs to, including for reconciliation (2026-11-22).

**Posted date**:
The date the institution posted a feed row. Kept as evidence only, to explain differences between occurred-date balances and a statement's ending balance.

**In-flight item**:
A line whose occurred and posted dates fall on opposite sides of a statement cutoff. In-flight items are the only allowed difference between an occurred-date balance and the statement's ending balance.
