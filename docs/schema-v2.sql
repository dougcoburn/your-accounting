-- =============================================================================
-- your-accounting kernel schema v2 (migration 1)
-- =============================================================================
-- Status: Proposed (U15, 2026-11-22). Replaces docs/schema-v0.2.sql.
-- U17 (2026-11-26): adds kernel_state and commits.kind 'freeze' (kernel spec §3.13).
-- U17 review: identity is the singleton entity row (ADR 0004/0008 amendment
-- 2026-11-26). ledger_accounts and journal_postings do not carry entity_id.
-- Fresh migration 1: no real database exists, so there is no data migration
-- from v0.2. The SPEC "upgraded DB" acceptance runs start at v2.
--
-- Applies:
--   ADR 0002 + amendment  append-only journal; recorded_at, reverses_id, and a
--                         set-once superseded_by flag; atomic multi-event commits
--   ADR 0003              one Party concept; entity.party_id; user access grants;
--                         no entities.visibility, no parties.opaque_entity_id
--   ADR 0004              one database per entity (this file is ONE entity's
--                         database); globally unique ULID ids; cross-entity
--                         recipe halves on accounting_events (rewritten
--                         2026-11-26: no outbox/inbox); evidence per entity database
--   ADR 0005              tracking mode per financial account; valuation
--                         snapshots on non-cash asset accounts only
--   ADR 0006              settlement exclusivity per posting; drafts never post
--   ADR 0007              observation natural key = feed id + external id
--   ADR 0012 (accepted 2026-11-27) identity is feed id + external id + financial account.
--                         A later pull appends observation_revisions. The first
--                         observations row is never updated.
--   ADR 0008              typed ledger accounts; categories are income/expense
--                         ledger accounts; no categories/allocations tables
--   2026-11-22 decision   the occurred date decides the period for everything,
--                         including reconciliation; the posted date is kept on
--                         observations only to explain in-flight items
--
-- Conventions
--   * Requires SQLite >= 3.37 (STRICT tables) and PRAGMA foreign_keys=ON on
--     every connection (rusqlite's bundled libsqlite3-sys 0.30 is 3.46).
--     The pragma is per connection and is NOT stored in the file: the kernel's
--     open() must assert PRAGMA foreign_keys = 1 and refuse to write otherwise
--     (SPEC-v2-delta §3). A connection without it can leave unsealed rows;
--     doctor reports them via doctor_unsealed_rows and PRAGMA foreign_key_check.
--   * Ids: 26-char Crockford base32 ULIDs, unique across ALL entity databases
--     (ADR 0004). Foreign keys never cross databases; a counterpart entity is
--     referenced through its party id (ADR 0003). Id source and clock are
--     injected by the application so fixture runs are deterministic (ADR 0002).
--     Exception: recipes(id) is a stable code identifier such as 'card_purchase'.
--   * Money: INTEGER minor units plus an ISO 4217 code. Posting and other
--     magnitudes are positive integers with an explicit side or direction;
--     only balance/value columns carry a sign (MAP 2026-11-19).
--   * Dates: 'YYYY-MM-DD'. Timestamps (recorded_at): UTC ISO 8601 ending in 'Z'.
--   * Every journal-side row belongs to a commit. A commit is the unit of
--     atomicity: the application writes all rows of a commit and inserts the
--     `commits` row LAST, in one SQLite transaction. The BEFORE INSERT trigger
--     on `commits` is the commit validator's storage-level backstop (INV-001,
--     ADR 0002, INV-006, INV-008). commit_id foreign keys are DEFERRABLE
--     INITIALLY DEFERRED, so a transaction that never writes its commits row
--     fails at COMMIT and leaves no partial state (AT-013).
--   * Rules a trigger cannot express cheaply (closed-month assertion recompute,
--     transitive correction validation INV-013, cross-database pairing) belong
--     to the commit validator (U19) and `doctor` (U21). They are listed at the
--     end of this file.
-- =============================================================================

PRAGMA foreign_keys = ON;
PRAGMA user_version = 1;

CREATE TABLE schema_migrations (
    version     INTEGER PRIMARY KEY CHECK (version >= 1),
    applied_at  TEXT NOT NULL CHECK (applied_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- -----------------------------------------------------------------------------
-- 1. This database's entity, commits, access grants, parties (ADR 0002-0004)
-- -----------------------------------------------------------------------------
-- Exactly one row: the entity whose books this database holds (ADR 0004).
-- party_id is the entity's stable party identity, the same ULID in every
-- database; another entity's database refers to this entity only as a party
-- row with that id (ADR 0003). There is deliberately NO visibility column.
CREATE TABLE entity (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    party_id    TEXT NOT NULL UNIQUE CHECK (length(party_id) = 26 AND party_id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*' AND party_id <> id),
    name        TEXT NOT NULL CHECK (length(name) > 0),
    created_at  TEXT NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    singleton   INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK (singleton = 1)   -- ADR 0004: one entity per database
) STRICT;

-- Commits: the append-only unit of change (ADR 0002). seq is the entity's
-- revision. expected_revision is the revision the CALLER saw (supplied to
-- begin_transaction, never re-read inside the transaction); the seal rejects the
-- commit unless it equals the current max(seq) (AT-032), and seq is always
-- expected_revision + 1. Written LAST in its transaction.
CREATE TABLE commits (
    id                TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    expected_revision INTEGER NOT NULL CHECK (expected_revision >= 0),
    seq               INTEGER NOT NULL UNIQUE CHECK (seq >= 1),
    kind        TEXT NOT NULL CHECK (kind IN ('setup','import','interpret','settle','correct','assert','cross_entity','grant','freeze')),
    actor       TEXT NOT NULL CHECK (length(actor) > 0),
    reason      TEXT NOT NULL DEFAULT '',
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z' AND julianday(recorded_at) IS NOT NULL),
    CHECK (seq = expected_revision + 1)
) STRICT;

-- U17 / kernel spec §3.13: the bugcheck write freeze. The live state is the
-- latest row, by its commit's seq, whose state_key is 'freeze'. active = 1
-- freezes writes; active = 0 is a clear and is a new row, never an update.
-- No amounts, names, memos, or payloads. commit_id is deferred so the row
-- cannot persist without its commits row (kind 'freeze').
CREATE TABLE kernel_state (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    state_key   TEXT NOT NULL CHECK (state_key = 'freeze'),
    active      INTEGER NOT NULL CHECK (active IN (0, 1)),
    code        TEXT NOT NULL CHECK (length(code) > 0),
    commit_id   TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z' AND julianday(recorded_at) IS NOT NULL)
) STRICT;
CREATE INDEX kernel_state_by_commit ON kernel_state(commit_id);

-- ADR 0003: users are granted access to entities. Append-only; a revocation is
-- a new row that points at the grant it revokes. user_id is a global ULID.
CREATE TABLE user_grants (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    user_id     TEXT NOT NULL CHECK (length(user_id) = 26 AND user_id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    role        TEXT NOT NULL CHECK (role IN ('owner','editor','viewer','revoke')),
    revokes_id  TEXT UNIQUE REFERENCES user_grants(id),
    commit_id   TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    CHECK ((role = 'revoke') = (revokes_id IS NOT NULL))
) STRICT;

-- ADR 0003: one Party concept. Anyone outside these books: merchants,
-- employers, people, and other entities (whose party row id equals that
-- entity's entity.party_id). label is UI wording only.
CREATE TABLE parties (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    name        TEXT NOT NULL CHECK (length(name) > 0),
    label       TEXT NOT NULL DEFAULT 'party' CHECK (label IN ('party','vendor','person','organization','employer')),
    parent_id   TEXT REFERENCES parties(id),
    created_at  TEXT NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

CREATE TABLE party_aliases (
    party_id    TEXT NOT NULL REFERENCES parties(id),
    alias       TEXT NOT NULL CHECK (length(alias) > 0),
    PRIMARY KEY (party_id, alias)
) STRICT;

-- -----------------------------------------------------------------------------
-- 2. Recipe registry (v0.2 "scenarios"; renamed per MAP repo-drift note)
-- -----------------------------------------------------------------------------
-- Historical events keep the recipe id/version that produced them (AT-012).
CREATE TABLE recipes (
    id          TEXT NOT NULL CHECK (length(id) > 0),
    version     TEXT NOT NULL CHECK (length(version) > 0),
    PRIMARY KEY (id, version)
) STRICT;

-- PR 19 Q5: append-only display-name history for reference rows (entity,
-- parties, financial/ledger accounts, tags, feeds). Not commit-scoped, like the
-- reference rows themselves. Party aliases stay in party_aliases.
CREATE TABLE reference_renames (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('entity','party','financial_account','ledger_account','tag','feed')),
    target_id   TEXT NOT NULL CHECK (length(target_id) = 26 AND target_id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    name        TEXT NOT NULL CHECK (length(name) > 0),
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;
CREATE INDEX reference_renames_target ON reference_renames(target_kind, target_id, recorded_at);

-- -----------------------------------------------------------------------------
-- 3. Financial accounts and typed ledger accounts (ADR 0005, ADR 0008)
-- -----------------------------------------------------------------------------
-- Every financial account in this database is owned by this entity (ADR 0004),
-- so v0.2's owner_entity_id and the account_owner_valid trigger are gone.
-- last4 only: the data guard blocks longer account numbers in repos (plan U01).
CREATE TABLE financial_accounts (
    id            TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    name          TEXT NOT NULL CHECK (length(name) > 0),
    kind          TEXT NOT NULL CHECK (kind IN ('checking','savings','cash','gift_card','store_credit','credit_card','loan','brokerage','retirement','hsa','real_estate','vehicle','other_asset','other_liability')),
    currency      TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),   -- INV-004: one currency
    tracking_mode TEXT NOT NULL CHECK (tracking_mode IN ('transparent','black_box')),          -- ADR 0005: fixed at creation in month 1
    institution   TEXT,
    last4         TEXT CHECK (last4 IS NULL OR (length(last4) = 4 AND last4 NOT GLOB '*[^0-9]*')),
    created_at    TEXT NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- Accepted (ADR 0012, 2026-11-27): a non-financial role for feed rows that are stored and
-- never posted. Month 1 has one role, payroll. The Monarch account name that
-- binds to it is import configuration, not a column here. One row per role.
CREATE TABLE cross_check_accounts (
    id         TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    role       TEXT NOT NULL UNIQUE CHECK (role = 'payroll'),
    created_at TEXT NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- ADR 0008: exactly one account_type; engine sub-kind in `kind`; categories are
-- the income/expense accounts (kind 'category'), a tree via parent_id (same
-- type) where only leaves take postings (INV-003). No entity_id column: every
-- row belongs to this database's entity (ADR 0004/0008 amendment 2026-11-26).
-- No 'contra' type or role: contra amounts are opposite-side postings.
-- Categories are currency-independent (SPEC §2.3), so there is no currency here.
CREATE TABLE ledger_accounts (
    id                   TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    financial_account_id TEXT REFERENCES financial_accounts(id),
    account_type         TEXT NOT NULL CHECK (account_type IN ('asset','liability','equity','income','expense')),
    kind                 TEXT NOT NULL CHECK (kind IN ('cash','card','loan','investment','fixed_asset','receivable','payable','clearing','opening_equity','equity','category')),
    parent_id            TEXT REFERENCES ledger_accounts(id),
    name                 TEXT NOT NULL CHECK (length(name) > 0),
    created_at           TEXT NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    -- ADR 0008: categories are exactly the income/expense accounts
    CHECK ((kind = 'category') = (account_type IN ('income','expense'))),
    -- ADR 0008: only categories form a tree
    CHECK (parent_id IS NULL OR kind = 'category'),
    CHECK (parent_id IS NULL OR parent_id <> id),
    -- ADR 0008: kind is consistent with the account type
    CHECK (kind NOT IN ('cash','investment','fixed_asset','receivable') OR account_type = 'asset'),
    CHECK (kind NOT IN ('card','loan','payable') OR account_type = 'liability'),
    CHECK (kind NOT IN ('opening_equity','equity') OR account_type = 'equity'),
    CHECK (kind <> 'clearing' OR account_type IN ('asset','liability')),
    -- ADR 0008: only balance-sheet account kinds are backed by a financial account
    CHECK (financial_account_id IS NULL OR kind IN ('cash','card','loan','investment','fixed_asset'))
) STRICT;
CREATE INDEX ledger_accounts_children ON ledger_accounts(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX ledger_accounts_financial ON ledger_accounts(financial_account_id) WHERE financial_account_id IS NOT NULL;

-- -----------------------------------------------------------------------------
-- 4. Append-only journal (ADR 0002 + amendment, ADR 0008)
-- -----------------------------------------------------------------------------
-- effective_date is the occurred date (2026-11-22): it alone assigns the event
-- to a period, including for reconciliation. Corrections and voids are
-- effective-dated to the underlying observation's date (ADR 0002), so a
-- reversal carries its target's effective_date (trigger below).
-- v0.2's posting_date / effective_date_override pair is gone.
-- superseded_by is the ONLY column ever written after insert: NULL -> id of the
-- event that reverses this one, inside that reversal's own commit, once
-- (ADR 0002 amendment). Balances never read it; doctor rebuilds it.
CREATE TABLE accounting_events (
    id                 TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    commit_id          TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recipe_id          TEXT NOT NULL,
    recipe_version     TEXT NOT NULL,
    -- ADR 0004 (rewritten 2026-11-26): one half of a cross-entity recipe. The
    -- application writes both halves; recipe_instance_id links them. recipe_id
    -- above stays the recipe code. Both are NULL or both are set. The amounts the
    -- other half must carry are rows in event_counterpart_expectations, one per
    -- currency (multi-currency halves are allowed; INV-001 stays per currency).
    recipe_instance_id TEXT CHECK (recipe_instance_id IS NULL OR (length(recipe_instance_id) = 26 AND recipe_instance_id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*')),
    counterpart_entity_id TEXT CHECK (counterpart_entity_id IS NULL OR (length(counterpart_entity_id) = 26 AND counterpart_entity_id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*')),
    effective_date     TEXT NOT NULL CHECK (date(effective_date) IS effective_date),
    description        TEXT NOT NULL DEFAULT '',
    recorded_at        TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z' AND julianday(recorded_at) IS NOT NULL),
    reverses_id        TEXT REFERENCES accounting_events(id),
    superseded_by      TEXT REFERENCES accounting_events(id),
    FOREIGN KEY (recipe_id, recipe_version) REFERENCES recipes(id, version),
    CHECK (reverses_id IS NULL OR reverses_id <> id),
    CHECK (superseded_by IS NULL OR superseded_by <> id),
    CHECK ((recipe_instance_id IS NULL) = (counterpart_entity_id IS NULL)),  -- ADR 0004: both or neither
    CHECK (reverses_id IS NULL OR recipe_instance_id IS NULL)               -- ADR 0004: a reversal carries no recipe fields
) STRICT;
-- ADR 0002: an event is reversed at most once.
CREATE UNIQUE INDEX accounting_events_reverses ON accounting_events(reverses_id) WHERE reverses_id IS NOT NULL;
-- ADR 0002 amendment: the partial index that serves "active only" register views.
CREATE INDEX accounting_events_register ON accounting_events(effective_date) WHERE superseded_by IS NULL AND reverses_id IS NULL;
CREATE INDEX accounting_events_commit ON accounting_events(commit_id);
CREATE INDEX accounting_events_recipe_instance ON accounting_events(recipe_instance_id) WHERE recipe_instance_id IS NOT NULL;
-- ADR 0004: at most one CURRENT half per recipe instance in this database. A
-- correction sets superseded_by on the target before inserting the replacement
-- that keeps the id, so the two are never current together.
CREATE UNIQUE INDEX accounting_events_recipe_instance_current ON accounting_events(recipe_instance_id) WHERE recipe_instance_id IS NOT NULL AND superseded_by IS NULL;

-- ADR 0004: what a recipe half expects the counterpart half to carry, per
-- currency, on the intercompany receivable/payable for this entity. Written
-- with its event (commit-scoped through the parent). A recipe half needs at
-- least one row (commits_seal); a reversal or a plain event has none.
CREATE TABLE event_counterpart_expectations (
    event_id              TEXT NOT NULL REFERENCES accounting_events(id),
    currency              TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),
    expected_amount_minor INTEGER NOT NULL CHECK (expected_amount_minor > 0),
    PRIMARY KEY (event_id, currency)
) STRICT;

-- INV-002: one ledger account, currency, positive magnitude, explicit side. No
-- entity_id: the database is the entity (ADR 0004 amendment 2026-11-26).
-- ADR 0008: P&L lines are ordinary postings; INV-001 closes from this table alone.
CREATE TABLE journal_postings (
    id                TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    event_id          TEXT NOT NULL REFERENCES accounting_events(id),
    ledger_account_id TEXT NOT NULL REFERENCES ledger_accounts(id),
    currency          TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),
    side              TEXT NOT NULL CHECK (side IN ('debit','credit')),
    amount_minor      INTEGER NOT NULL CHECK (amount_minor > 0),
    party_id          TEXT REFERENCES parties(id),
    memo              TEXT NOT NULL DEFAULT ''
) STRICT;
CREATE INDEX journal_postings_event ON journal_postings(event_id, currency, side);
CREATE INDEX journal_postings_account ON journal_postings(ledger_account_id, event_id);

-- ADR 0008: tags attach to postings (replaces allocation_tags). Filter predicates only.
CREATE TABLE tags (
    id   TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    name TEXT NOT NULL UNIQUE CHECK (length(name) > 0)
) STRICT;
CREATE TABLE posting_tags (
    posting_id TEXT NOT NULL REFERENCES journal_postings(id),
    tag_id     TEXT NOT NULL REFERENCES tags(id),
    PRIMARY KEY (posting_id, tag_id)
) STRICT;

-- ADR 0008 / MAP 2026-11-19: a refund is an opposite-side posting to the
-- purchase's own income/expense account, optionally linked to the purchase
-- posting(s). INV-015 cap enforced by trigger.
CREATE TABLE refund_links (
    id                  TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    refund_posting_id   TEXT NOT NULL REFERENCES journal_postings(id),
    purchase_posting_id TEXT NOT NULL REFERENCES journal_postings(id),
    amount_minor        INTEGER NOT NULL CHECK (amount_minor > 0),
    UNIQUE (refund_posting_id, purchase_posting_id),
    CHECK (refund_posting_id <> purchase_posting_id)
) STRICT;

-- -----------------------------------------------------------------------------
-- 5. Evidence, per entity database (ADR 0004; SPEC §8, INV-014, INV-020)
-- -----------------------------------------------------------------------------
CREATE TABLE evidence_objects (
    sha256      TEXT PRIMARY KEY CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    size_bytes  INTEGER NOT NULL CHECK (size_bytes >= 0),
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;
CREATE TABLE evidence_documents (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    sha256      TEXT NOT NULL REFERENCES evidence_objects(sha256),
    filename    TEXT,
    media_type  TEXT,
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;
-- INV-020: deleting bytes is a tombstone; hash, size, links and metadata remain.
-- Present/deleted status is derived (v0.2's mutable status column is gone).
CREATE TABLE evidence_deletions (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    sha256      TEXT NOT NULL UNIQUE REFERENCES evidence_objects(sha256),
    reason      TEXT NOT NULL CHECK (length(reason) > 0),
    commit_id   TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- -----------------------------------------------------------------------------
-- 6. Feeds and observations (ADR 0007, ADR 0002 void, 2026-11-22 dates)
-- -----------------------------------------------------------------------------
CREATE TABLE feeds (
    id    TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    kind  TEXT NOT NULL CHECK (kind IN ('monarch_csv','bankstmt_json','manual')),
    name  TEXT NOT NULL CHECK (length(name) > 0)
) STRICT;
CREATE TABLE import_runs (
    id               TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    feed_id          TEXT NOT NULL REFERENCES feeds(id),
    document_id      TEXT REFERENCES evidence_documents(id),
    importer_id      TEXT NOT NULL,
    importer_version TEXT NOT NULL,
    commit_id        TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at      TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- An observation's first snapshot is an immutable accepted source fact
-- (SPEC §2.5, INV-014). Identity = (feed_id, external_id, financial_account_id)
-- or, for a payroll cross-check row, (feed_id, external_id, cross_check_account_id)
-- (ADR 0007, ADR 0012 accepted 2026-11-27). Exactly one of those account columns is set.
-- A later pull of the same identity appends
-- observation_revisions and does not update this row. Current attributes are
-- the view observation_current. An identical row_hash is a no-op.
-- occurred_date assigns the period; posted_date is optional and only explains
-- in-flight items against a statement (2026-11-22). direction is the sign of
-- the source row; amount_minor is unsigned. payload_json lives only in the
-- local per-entity database, never in a repo (plan Ground rules).
CREATE TABLE observations (
    id                   TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    feed_id              TEXT NOT NULL REFERENCES feeds(id),
    external_id          TEXT NOT NULL CHECK (length(external_id) > 0),
    import_run_id        TEXT REFERENCES import_runs(id),
    financial_account_id   TEXT REFERENCES financial_accounts(id),
    cross_check_account_id TEXT REFERENCES cross_check_accounts(id),
    occurred_date        TEXT NOT NULL CHECK (date(occurred_date) IS occurred_date),
    posted_date          TEXT CHECK (posted_date IS NULL OR date(posted_date) IS posted_date),
    direction            TEXT NOT NULL CHECK (direction IN ('inflow','outflow')),
    amount_minor         INTEGER NOT NULL CHECK (amount_minor > 0),
    currency             TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),
    description          TEXT NOT NULL DEFAULT '',
    row_hash             TEXT NOT NULL CHECK (length(row_hash) = 64 AND row_hash NOT GLOB '*[^0-9a-f]*'),
    payload_json         TEXT NOT NULL CHECK (json_valid(payload_json)),
    corroborates_id      TEXT REFERENCES observations(id),   -- ADR 0007: a later row for the same movement corroborates, never duplicates
    commit_id            TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at          TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    CHECK (corroborates_id IS NULL OR corroborates_id <> id),
    -- Exactly one binding: a financial account, or a cross-check role.
    CHECK ((financial_account_id IS NULL) = (cross_check_account_id IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX observations_feed_financial
    ON observations(feed_id, external_id, financial_account_id)
    WHERE financial_account_id IS NOT NULL;
CREATE UNIQUE INDEX observations_feed_cross_check
    ON observations(feed_id, external_id, cross_check_account_id)
    WHERE cross_check_account_id IS NOT NULL;
CREATE INDEX observations_account_date ON observations(financial_account_id, occurred_date);

-- Voiding is an import-context tombstone (Doug, 2026-11-27). It ends this
-- observation's live links and does not reverse or correct kernel events.
-- posting_observations rows stay; a link is live only while its observation
-- has no row here. The commit, when one is required, is a non-reversal kind.
CREATE TABLE observation_voids (
    id             TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    observation_id TEXT NOT NULL UNIQUE REFERENCES observations(id),
    reason         TEXT NOT NULL CHECK (reason IN ('duplicate','extraction_error','other')),
    commit_id      TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at    TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- Which financial-account-side posting interprets which observation (replaces
-- v0.2 canonical_transactions / account_movements / movement_observations).
-- INV-007 among current events is checked by doctor_observation_double_consumed.
CREATE TABLE posting_observations (
    posting_id     TEXT NOT NULL REFERENCES journal_postings(id),
    observation_id TEXT NOT NULL REFERENCES observations(id),
    PRIMARY KEY (posting_id, observation_id)
) STRICT;
CREATE INDEX posting_observations_observation ON posting_observations(observation_id);

-- Accepted (ADR 0012, 2026-11-27): each later pull of the same identity appends the new
-- source attributes, including the raw payload. The observations row stays
-- the first snapshot. Nothing here is updated or deleted.
CREATE TABLE observation_revisions (
    id             TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    observation_id TEXT NOT NULL REFERENCES observations(id),
    import_run_id  TEXT REFERENCES import_runs(id),
    occurred_date  TEXT NOT NULL CHECK (date(occurred_date) IS occurred_date),
    posted_date    TEXT CHECK (posted_date IS NULL OR date(posted_date) IS posted_date),
    direction      TEXT NOT NULL CHECK (direction IN ('inflow','outflow')),
    amount_minor   INTEGER NOT NULL CHECK (amount_minor > 0),
    currency       TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),
    description    TEXT NOT NULL DEFAULT '',
    row_hash       TEXT NOT NULL CHECK (length(row_hash) = 64 AND row_hash NOT GLOB '*[^0-9a-f]*'),
    payload_json   TEXT NOT NULL CHECK (json_valid(payload_json)),
    commit_id      TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at    TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;
CREATE INDEX observation_revisions_observation ON observation_revisions(observation_id, recorded_at);

-- Current source attributes: the latest revision, or the first snapshot.
CREATE VIEW observation_current AS
SELECT o.id, o.feed_id, o.external_id, o.financial_account_id, o.cross_check_account_id, o.corroborates_id,
       CASE WHEN r.id IS NULL THEN o.occurred_date ELSE r.occurred_date END AS occurred_date,
       CASE WHEN r.id IS NULL THEN o.posted_date ELSE r.posted_date END AS posted_date,
       CASE WHEN r.id IS NULL THEN o.direction ELSE r.direction END AS direction,
       CASE WHEN r.id IS NULL THEN o.amount_minor ELSE r.amount_minor END AS amount_minor,
       CASE WHEN r.id IS NULL THEN o.currency ELSE r.currency END AS currency,
       CASE WHEN r.id IS NULL THEN o.description ELSE r.description END AS description,
       CASE WHEN r.id IS NULL THEN o.row_hash ELSE r.row_hash END AS row_hash,
       CASE WHEN r.id IS NULL THEN o.payload_json ELSE r.payload_json END AS payload_json
FROM observations o
LEFT JOIN observation_revisions r ON r.id = (
    SELECT r2.id FROM observation_revisions r2
    WHERE r2.observation_id = o.id
    ORDER BY r2.recorded_at DESC, r2.id DESC
    LIMIT 1
);

-- Document-level evidence links (SPEC §2.5). Exactly one target.
CREATE TABLE evidence_links (
    id             TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    document_id    TEXT NOT NULL REFERENCES evidence_documents(id),
    event_id       TEXT REFERENCES accounting_events(id),
    observation_id TEXT REFERENCES observations(id),
    commit_id      TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    CHECK ((event_id IS NOT NULL) <> (observation_id IS NOT NULL))
) STRICT;

-- -----------------------------------------------------------------------------
-- 7. Settlement groups: append-only, state derived (ADR 0006, INV-006/008)
-- -----------------------------------------------------------------------------
-- v0.2's mutable state/revision columns are gone. A group's state is its
-- latest settlement_group_states row: draft (repeatable) -> reconciled ->
-- dissolved, or draft -> dissolved.
CREATE TABLE settlement_groups (
    id                 TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    recipe_id          TEXT NOT NULL,
    recipe_version     TEXT NOT NULL,
    recipe_instance_id TEXT CHECK (recipe_instance_id IS NULL OR (length(recipe_instance_id) = 26 AND recipe_instance_id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*')),
    commit_id          TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at        TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    FOREIGN KEY (recipe_id, recipe_version) REFERENCES recipes(id, version)
) STRICT;
CREATE TABLE settlement_group_states (
    id          TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    group_id    TEXT NOT NULL REFERENCES settlement_groups(id),
    seq         INTEGER NOT NULL CHECK (seq >= 1),
    state       TEXT NOT NULL CHECK (state IN ('draft','reconciled','dissolved')),
    commit_id   TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    UNIQUE (group_id, seq)
) STRICT;
-- Participants of a draft state (observed lines or expected slots). Drafts
-- have no postings (INV-006); an expected slot is a descriptor, never a
-- committed clearing entry (ADR 0006).
CREATE TABLE settlement_draft_participants (
    id             TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    group_state_id TEXT NOT NULL REFERENCES settlement_group_states(id),
    role           TEXT NOT NULL CHECK (length(role) > 0),
    observation_id TEXT REFERENCES observations(id),
    expected_json  TEXT CHECK (expected_json IS NULL OR json_valid(expected_json)),
    CHECK ((observation_id IS NOT NULL) <> (expected_json IS NOT NULL))
) STRICT;
-- ADR 0006 settlement exclusivity: a posting line belongs to at most one group,
-- ever (corrections reverse and repost, so a new posting joins the new group).
CREATE TABLE settlement_postings (
    group_id   TEXT NOT NULL REFERENCES settlement_groups(id),
    posting_id TEXT NOT NULL UNIQUE REFERENCES journal_postings(id),
    commit_id  TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    PRIMARY KEY (group_id, posting_id)
) STRICT;

-- -----------------------------------------------------------------------------
-- 8. Obligations / reimbursement claims (ADR 0001)
-- -----------------------------------------------------------------------------
-- The counterparty is always a party (ADR 0003), including another entity.
-- Remaining amount is derived: original - sum(current settlements).
CREATE TABLE obligations (
    id                TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    party_id          TEXT NOT NULL REFERENCES parties(id),
    direction         TEXT NOT NULL CHECK (direction IN ('receivable','payable')),
    ledger_account_id TEXT NOT NULL REFERENCES ledger_accounts(id),
    currency          TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),
    original_minor    INTEGER NOT NULL CHECK (original_minor > 0),
    opened_event_id   TEXT NOT NULL REFERENCES accounting_events(id),
    commit_id         TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at       TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;
-- ADR 0001: a claim links to the specific purchase posting(s) it came from.
CREATE TABLE obligation_postings (
    obligation_id TEXT NOT NULL REFERENCES obligations(id),
    posting_id    TEXT NOT NULL REFERENCES journal_postings(id),
    PRIMARY KEY (obligation_id, posting_id)
) STRICT;
-- ADR 0001: partial settlement allowed; forgiveness is a new event whose
-- category the user picks at decision time (INV-012). Cap enforced by trigger.
CREATE TABLE obligation_settlements (
    id            TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    obligation_id TEXT NOT NULL REFERENCES obligations(id),
    event_id      TEXT NOT NULL REFERENCES accounting_events(id),
    kind          TEXT NOT NULL CHECK (kind IN ('payment','forgiveness')),
    amount_minor  INTEGER NOT NULL CHECK (amount_minor > 0),
    commit_id     TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at   TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- -----------------------------------------------------------------------------
-- 9. Statements, balance assertions, valuation snapshots (SPEC §6, ADR 0005)
-- -----------------------------------------------------------------------------
CREATE TABLE statements (
    id                   TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    financial_account_id TEXT NOT NULL REFERENCES financial_accounts(id),
    period_start         TEXT NOT NULL CHECK (date(period_start) IS period_start),
    period_end           TEXT NOT NULL CHECK (date(period_end) IS period_end),
    document_id          TEXT REFERENCES evidence_documents(id),
    commit_id            TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at          TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    CHECK (period_start <= period_end)
) STRICT;
-- balance_minor is signed in the account's normal direction: positive = asset
-- held or liability owed; a card credit balance is negative.
-- cutoff is compared against occurred dates (2026-11-22); the statement's
-- ending balance is explained by in-flight items via observation posted dates (U32).
-- INV-011: never changed; a revision is a new row with supersedes_id (approved correction).
CREATE TABLE balance_assertions (
    id                   TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    financial_account_id TEXT NOT NULL REFERENCES financial_accounts(id),
    cutoff               TEXT NOT NULL CHECK (date(cutoff) IS cutoff),
    balance_minor        INTEGER NOT NULL,
    currency             TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),
    source               TEXT NOT NULL CHECK (source IN ('manual','statement')),   -- AT-024 parity: same arithmetic either way
    statement_id         TEXT REFERENCES statements(id),
    document_id          TEXT REFERENCES evidence_documents(id),
    supersedes_id        TEXT UNIQUE REFERENCES balance_assertions(id),
    commit_id            TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at          TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    CHECK ((source = 'statement') = (statement_id IS NOT NULL))
) STRICT;
CREATE INDEX balance_assertions_account ON balance_assertions(financial_account_id, cutoff);

-- ADR 0005: market value outside the journal, never a posting, only on
-- non-cash asset accounts (trigger below); never on cash, card or liability
-- accounts, so their balance checks stay meaningful (INV-011).
CREATE TABLE valuation_snapshots (
    id                   TEXT PRIMARY KEY CHECK (length(id) = 26 AND id NOT GLOB '*[^0-9A-HJKMNP-TV-Z]*'),
    financial_account_id TEXT NOT NULL REFERENCES financial_accounts(id),
    as_of                TEXT NOT NULL CHECK (date(as_of) IS as_of),
    value_minor          INTEGER NOT NULL CHECK (value_minor >= 0),
    currency             TEXT NOT NULL CHECK (length(currency) = 3 AND currency NOT GLOB '*[^A-Z]*'),
    source               TEXT NOT NULL CHECK (source IN ('statement','appraisal','estimate','manual')),
    document_id          TEXT REFERENCES evidence_documents(id),
    commit_id            TEXT NOT NULL REFERENCES commits(id) DEFERRABLE INITIALLY DEFERRED,
    recorded_at          TEXT NOT NULL CHECK (recorded_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- =============================================================================
-- TRIGGERS
-- =============================================================================

-- --- Reference data: immutable, never deleted; renames are appended ---------
-- PR 19 Q5: display names never change in place, so replay and audit show what
-- a row was called at the time. A rename is a reference_renames row (below);
-- the current name is the latest rename, else the row's original name.
CREATE TRIGGER entity_no_update BEFORE UPDATE ON entity
BEGIN SELECT RAISE(ABORT, 'ADR 0003/0004: entity is immutable; rename via reference_renames'); END;
CREATE TRIGGER entity_no_delete BEFORE DELETE ON entity
BEGIN SELECT RAISE(ABORT, 'ADR 0004: the entity row is never deleted'); END;
CREATE TRIGGER parties_no_update BEFORE UPDATE ON parties
BEGIN SELECT RAISE(ABORT, 'ADR 0003: parties are immutable; rename via reference_renames or add a party_aliases row'); END;
CREATE TRIGGER parties_no_delete BEFORE DELETE ON parties
BEGIN SELECT RAISE(ABORT, 'ADR 0003: parties are never deleted'); END;
CREATE TRIGGER financial_accounts_no_update BEFORE UPDATE ON financial_accounts
BEGIN SELECT RAISE(ABORT, 'INV-004/ADR 0005: financial accounts are immutable (mode switch is a deferred wizard); rename via reference_renames'); END;
CREATE TRIGGER financial_accounts_no_delete BEFORE DELETE ON financial_accounts
BEGIN SELECT RAISE(ABORT, 'ADR 0002: financial accounts are never deleted'); END;
-- ADR 0008: moving history between categories is a correction; re-parenting is deferred.
CREATE TRIGGER ledger_accounts_no_update BEFORE UPDATE ON ledger_accounts
BEGIN SELECT RAISE(ABORT, 'ADR 0008: ledger accounts are immutable; rename via reference_renames'); END;
CREATE TRIGGER ledger_accounts_no_delete BEFORE DELETE ON ledger_accounts
BEGIN SELECT RAISE(ABORT, 'ADR 0002: ledger accounts are never deleted'); END;
CREATE TRIGGER tags_no_update BEFORE UPDATE ON tags
BEGIN SELECT RAISE(ABORT, 'tags are immutable; rename via reference_renames'); END;
CREATE TRIGGER tags_no_delete BEFORE DELETE ON tags
BEGIN SELECT RAISE(ABORT, 'tags are never deleted'); END;
CREATE TRIGGER feeds_no_update BEFORE UPDATE ON feeds
BEGIN SELECT RAISE(ABORT, 'ADR 0007: feeds are immutable; rename via reference_renames'); END;
CREATE TRIGGER feeds_no_delete BEFORE DELETE ON feeds
BEGIN SELECT RAISE(ABORT, 'ADR 0007: feeds are never deleted'); END;
CREATE TRIGGER reference_renames_target_valid BEFORE INSERT ON reference_renames
WHEN NOT EXISTS (
       SELECT 1 FROM entity             WHERE NEW.target_kind = 'entity'            AND id = NEW.target_id
 UNION ALL SELECT 1 FROM parties            WHERE NEW.target_kind = 'party'             AND id = NEW.target_id
 UNION ALL SELECT 1 FROM financial_accounts WHERE NEW.target_kind = 'financial_account' AND id = NEW.target_id
 UNION ALL SELECT 1 FROM ledger_accounts    WHERE NEW.target_kind = 'ledger_account'    AND id = NEW.target_id
 UNION ALL SELECT 1 FROM tags               WHERE NEW.target_kind = 'tag'               AND id = NEW.target_id
 UNION ALL SELECT 1 FROM feeds              WHERE NEW.target_kind = 'feed'              AND id = NEW.target_id)
BEGIN SELECT RAISE(ABORT, 'PR 19 Q5: rename target does not exist'); END;

-- ADR 0008 / INV-003: a category's parent is a same-type category
-- that has no postings (postings only on leaves).
CREATE TRIGGER ledger_accounts_parent_valid BEFORE INSERT ON ledger_accounts
WHEN NEW.parent_id IS NOT NULL AND (
       (SELECT account_type FROM ledger_accounts WHERE id = NEW.parent_id) IS NOT NEW.account_type
    OR EXISTS (SELECT 1 FROM journal_postings WHERE ledger_account_id = NEW.parent_id))
BEGIN SELECT RAISE(ABORT, 'INV-003/ADR 0008: category parent must be a same-type category with no postings'); END;

-- ADR 0008: a financial-account-backed ledger account matches the account's nature.
CREATE TRIGGER ledger_accounts_financial_kind BEFORE INSERT ON ledger_accounts
WHEN NEW.financial_account_id IS NOT NULL AND NOT (
     (NEW.kind = 'cash'        AND (SELECT kind FROM financial_accounts WHERE id = NEW.financial_account_id) IN ('checking','savings','cash','gift_card','store_credit','other_asset'))
  OR (NEW.kind = 'card'        AND (SELECT kind FROM financial_accounts WHERE id = NEW.financial_account_id) = 'credit_card')
  OR (NEW.kind = 'loan'        AND (SELECT kind FROM financial_accounts WHERE id = NEW.financial_account_id) IN ('loan','other_liability'))
  OR (NEW.kind = 'investment'  AND (SELECT kind FROM financial_accounts WHERE id = NEW.financial_account_id) IN ('brokerage','retirement','hsa'))
  OR (NEW.kind = 'fixed_asset' AND (SELECT kind FROM financial_accounts WHERE id = NEW.financial_account_id) IN ('real_estate','vehicle','other_asset')))
BEGIN SELECT RAISE(ABORT, 'ADR 0008: ledger kind does not match the financial account kind'); END;


-- --- Append-only tables: no UPDATE, no DELETE (ADR 0002) ----------------------
-- accounting_events has its own UPDATE rule (superseded_by) further down.
CREATE TRIGGER commits_no_update BEFORE UPDATE ON commits BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: commits rows are never updated'); END;
CREATE TRIGGER commits_no_delete BEFORE DELETE ON commits BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: commits rows are never deleted'); END;
CREATE TRIGGER kernel_state_no_update BEFORE UPDATE ON kernel_state BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: kernel_state rows are never updated'); END;
CREATE TRIGGER kernel_state_no_delete BEFORE DELETE ON kernel_state BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: kernel_state rows are never deleted'); END;
CREATE TRIGGER schema_migrations_no_update BEFORE UPDATE ON schema_migrations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: schema_migrations rows are never updated'); END;
CREATE TRIGGER schema_migrations_no_delete BEFORE DELETE ON schema_migrations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: schema_migrations rows are never deleted'); END;
CREATE TRIGGER user_grants_no_update BEFORE UPDATE ON user_grants BEGIN SELECT RAISE(ABORT, 'ADR 0003/0002 append-only: user_grants rows are never updated'); END;
CREATE TRIGGER user_grants_no_delete BEFORE DELETE ON user_grants BEGIN SELECT RAISE(ABORT, 'ADR 0003/0002 append-only: user_grants rows are never deleted'); END;
CREATE TRIGGER party_aliases_no_update BEFORE UPDATE ON party_aliases BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: party_aliases rows are never updated'); END;
CREATE TRIGGER party_aliases_no_delete BEFORE DELETE ON party_aliases BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: party_aliases rows are never deleted'); END;
CREATE TRIGGER reference_renames_no_update BEFORE UPDATE ON reference_renames BEGIN SELECT RAISE(ABORT, 'PR 19 Q5 append-only: reference_renames rows are never updated'); END;
CREATE TRIGGER reference_renames_no_delete BEFORE DELETE ON reference_renames BEGIN SELECT RAISE(ABORT, 'PR 19 Q5 append-only: reference_renames rows are never deleted'); END;
CREATE TRIGGER recipes_no_update BEFORE UPDATE ON recipes BEGIN SELECT RAISE(ABORT, 'AT-012 append-only: recipes rows are never updated'); END;
CREATE TRIGGER recipes_no_delete BEFORE DELETE ON recipes BEGIN SELECT RAISE(ABORT, 'AT-012 append-only: recipes rows are never deleted'); END;
CREATE TRIGGER journal_postings_no_update BEFORE UPDATE ON journal_postings BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: journal_postings rows are never updated'); END;
CREATE TRIGGER journal_postings_no_delete BEFORE DELETE ON journal_postings BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: journal_postings rows are never deleted'); END;
CREATE TRIGGER posting_tags_no_update BEFORE UPDATE ON posting_tags BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: posting_tags rows are never updated'); END;
CREATE TRIGGER posting_tags_no_delete BEFORE DELETE ON posting_tags BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: posting_tags rows are never deleted'); END;
CREATE TRIGGER refund_links_no_update BEFORE UPDATE ON refund_links BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: refund_links rows are never updated'); END;
CREATE TRIGGER refund_links_no_delete BEFORE DELETE ON refund_links BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: refund_links rows are never deleted'); END;
CREATE TRIGGER evidence_objects_no_update BEFORE UPDATE ON evidence_objects BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_objects rows are never updated'); END;
CREATE TRIGGER evidence_objects_no_delete BEFORE DELETE ON evidence_objects BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_objects rows are never deleted'); END;
CREATE TRIGGER evidence_documents_no_update BEFORE UPDATE ON evidence_documents BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_documents rows are never updated'); END;
CREATE TRIGGER evidence_documents_no_delete BEFORE DELETE ON evidence_documents BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_documents rows are never deleted'); END;
CREATE TRIGGER evidence_deletions_no_update BEFORE UPDATE ON evidence_deletions BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_deletions rows are never updated'); END;
CREATE TRIGGER evidence_deletions_no_delete BEFORE DELETE ON evidence_deletions BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_deletions rows are never deleted'); END;
CREATE TRIGGER evidence_links_no_update BEFORE UPDATE ON evidence_links BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_links rows are never updated'); END;
CREATE TRIGGER evidence_links_no_delete BEFORE DELETE ON evidence_links BEGIN SELECT RAISE(ABORT, 'INV-020/ADR 0002 append-only: evidence_links rows are never deleted'); END;
CREATE TRIGGER import_runs_no_update BEFORE UPDATE ON import_runs BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: import_runs rows are never updated'); END;
CREATE TRIGGER import_runs_no_delete BEFORE DELETE ON import_runs BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: import_runs rows are never deleted'); END;
CREATE TRIGGER observations_no_update BEFORE UPDATE ON observations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: observations rows are never updated'); END;
CREATE TRIGGER observations_no_delete BEFORE DELETE ON observations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: observations rows are never deleted; void an observation with observation_voids'); END;
CREATE TRIGGER observation_revisions_no_update BEFORE UPDATE ON observation_revisions BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: observation_revisions rows are never updated'); END;
CREATE TRIGGER observation_revisions_no_delete BEFORE DELETE ON observation_revisions BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: observation_revisions rows are never deleted'); END;
CREATE TRIGGER observation_voids_no_update BEFORE UPDATE ON observation_voids BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: observation_voids rows are never updated'); END;
CREATE TRIGGER observation_voids_no_delete BEFORE DELETE ON observation_voids BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: observation_voids rows are never deleted'); END;
CREATE TRIGGER event_counterpart_expectations_no_update BEFORE UPDATE ON event_counterpart_expectations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: event_counterpart_expectations rows are never updated'); END;
CREATE TRIGGER event_counterpart_expectations_no_delete BEFORE DELETE ON event_counterpart_expectations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: event_counterpart_expectations rows are never deleted'); END;
CREATE TRIGGER posting_observations_no_update BEFORE UPDATE ON posting_observations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: posting_observations rows are never updated'); END;
CREATE TRIGGER posting_observations_no_delete BEFORE DELETE ON posting_observations BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: posting_observations rows are never deleted'); END;
CREATE TRIGGER settlement_groups_no_update BEFORE UPDATE ON settlement_groups BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_groups rows are never updated'); END;
CREATE TRIGGER settlement_groups_no_delete BEFORE DELETE ON settlement_groups BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_groups rows are never deleted'); END;
CREATE TRIGGER settlement_group_states_no_update BEFORE UPDATE ON settlement_group_states BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_group_states rows are never updated'); END;
CREATE TRIGGER settlement_group_states_no_delete BEFORE DELETE ON settlement_group_states BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_group_states rows are never deleted'); END;
CREATE TRIGGER settlement_draft_participants_no_update BEFORE UPDATE ON settlement_draft_participants BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_draft_participants rows are never updated'); END;
CREATE TRIGGER settlement_draft_participants_no_delete BEFORE DELETE ON settlement_draft_participants BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_draft_participants rows are never deleted'); END;
CREATE TRIGGER settlement_postings_no_update BEFORE UPDATE ON settlement_postings BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_postings rows are never updated'); END;
CREATE TRIGGER settlement_postings_no_delete BEFORE DELETE ON settlement_postings BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: settlement_postings rows are never deleted'); END;
CREATE TRIGGER obligations_no_update BEFORE UPDATE ON obligations BEGIN SELECT RAISE(ABORT, 'ADR 0001/0002 append-only: obligations rows are never updated'); END;
CREATE TRIGGER obligations_no_delete BEFORE DELETE ON obligations BEGIN SELECT RAISE(ABORT, 'ADR 0001/0002 append-only: obligations rows are never deleted'); END;
CREATE TRIGGER obligation_postings_no_update BEFORE UPDATE ON obligation_postings BEGIN SELECT RAISE(ABORT, 'ADR 0001/0002 append-only: obligation_postings rows are never updated'); END;
CREATE TRIGGER obligation_postings_no_delete BEFORE DELETE ON obligation_postings BEGIN SELECT RAISE(ABORT, 'ADR 0001/0002 append-only: obligation_postings rows are never deleted'); END;
CREATE TRIGGER obligation_settlements_no_update BEFORE UPDATE ON obligation_settlements BEGIN SELECT RAISE(ABORT, 'ADR 0001/0002 append-only: obligation_settlements rows are never updated'); END;
CREATE TRIGGER obligation_settlements_no_delete BEFORE DELETE ON obligation_settlements BEGIN SELECT RAISE(ABORT, 'ADR 0001/0002 append-only: obligation_settlements rows are never deleted'); END;
CREATE TRIGGER statements_no_update BEFORE UPDATE ON statements BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: statements rows are never updated'); END;
CREATE TRIGGER statements_no_delete BEFORE DELETE ON statements BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: statements rows are never deleted'); END;
CREATE TRIGGER balance_assertions_no_update BEFORE UPDATE ON balance_assertions BEGIN SELECT RAISE(ABORT, 'INV-011 append-only: balance_assertions rows are never updated'); END;
CREATE TRIGGER balance_assertions_no_delete BEFORE DELETE ON balance_assertions BEGIN SELECT RAISE(ABORT, 'INV-011 append-only: balance_assertions rows are never deleted'); END;
CREATE TRIGGER valuation_snapshots_no_update BEFORE UPDATE ON valuation_snapshots BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: valuation_snapshots rows are never updated'); END;
CREATE TRIGGER valuation_snapshots_no_delete BEFORE DELETE ON valuation_snapshots BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: valuation_snapshots rows are never deleted'); END;
CREATE TRIGGER accounting_events_no_delete BEFORE DELETE ON accounting_events BEGIN SELECT RAISE(ABORT, 'ADR 0002 append-only: accounting_events rows are never deleted'); END;

-- --- Sealed commits accept no new rows (ADR 0002 atomic commit) --------------
CREATE TRIGGER user_grants_open_commit BEFORE INSERT ON user_grants WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER accounting_events_open_commit BEFORE INSERT ON accounting_events WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER evidence_deletions_open_commit BEFORE INSERT ON evidence_deletions WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER evidence_links_open_commit BEFORE INSERT ON evidence_links WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER import_runs_open_commit BEFORE INSERT ON import_runs WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER observations_open_commit BEFORE INSERT ON observations WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER observation_revisions_open_commit BEFORE INSERT ON observation_revisions WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER observation_voids_open_commit BEFORE INSERT ON observation_voids WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER settlement_groups_open_commit BEFORE INSERT ON settlement_groups WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER settlement_group_states_open_commit BEFORE INSERT ON settlement_group_states WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER settlement_postings_open_commit BEFORE INSERT ON settlement_postings WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER obligations_open_commit BEFORE INSERT ON obligations WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER obligation_settlements_open_commit BEFORE INSERT ON obligation_settlements WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER statements_open_commit BEFORE INSERT ON statements WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER balance_assertions_open_commit BEFORE INSERT ON balance_assertions WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER valuation_snapshots_open_commit BEFORE INSERT ON valuation_snapshots WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
CREATE TRIGGER kernel_state_open_commit BEFORE INSERT ON kernel_state WHEN EXISTS (SELECT 1 FROM commits WHERE id = NEW.commit_id) BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed'); END;
-- Rows that hang off a posting, obligation or group state join their parent's commit.
CREATE TRIGGER journal_postings_open_commit BEFORE INSERT ON journal_postings
WHEN EXISTS (SELECT 1 FROM accounting_events e JOIN commits c ON c.id = e.commit_id WHERE e.id = NEW.event_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed (postings may only be added to an event in an open commit)'); END;
CREATE TRIGGER posting_tags_open_commit BEFORE INSERT ON posting_tags
WHEN EXISTS (SELECT 1 FROM journal_postings p JOIN accounting_events e ON e.id = p.event_id JOIN commits c ON c.id = e.commit_id WHERE p.id = NEW.posting_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed (tags are written with their posting; retagging is a correction)'); END;
CREATE TRIGGER event_counterpart_expectations_open_commit BEFORE INSERT ON event_counterpart_expectations
WHEN EXISTS (SELECT 1 FROM accounting_events e JOIN commits c ON c.id = e.commit_id WHERE e.id = NEW.event_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed (counterpart expectations are written with their event)'); END;
CREATE TRIGGER posting_observations_open_commit BEFORE INSERT ON posting_observations
WHEN EXISTS (SELECT 1 FROM journal_postings p JOIN accounting_events e ON e.id = p.event_id JOIN commits c ON c.id = e.commit_id WHERE p.id = NEW.posting_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed (an interpretation link is written with its posting)'); END;
CREATE TRIGGER refund_links_open_commit BEFORE INSERT ON refund_links
WHEN EXISTS (SELECT 1 FROM journal_postings p JOIN accounting_events e ON e.id = p.event_id JOIN commits c ON c.id = e.commit_id WHERE p.id = NEW.refund_posting_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed (a refund link is written with the refund posting; linking later is a correction)'); END;
CREATE TRIGGER obligation_postings_open_commit BEFORE INSERT ON obligation_postings
WHEN EXISTS (SELECT 1 FROM obligations o JOIN commits c ON c.id = o.commit_id WHERE o.id = NEW.obligation_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed (claim links are written with the claim, ADR 0001)'); END;
CREATE TRIGGER settlement_draft_participants_open_commit BEFORE INSERT ON settlement_draft_participants
WHEN EXISTS (SELECT 1 FROM settlement_group_states s JOIN commits c ON c.id = s.commit_id WHERE s.id = NEW.group_state_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: commit already sealed (draft participants are written with their group state)'); END;

-- --- Journal rules -----------------------------------------------------------
-- ADR 0002 amendment: superseded_by is the only mutable column. It may go from
-- NULL to the id of the event that reverses this one, only while that
-- reversal's commit is still open (same atomic commit), and never again.
CREATE TRIGGER accounting_events_only_supersede BEFORE UPDATE ON accounting_events
WHEN NEW.id IS NOT OLD.id OR NEW.commit_id IS NOT OLD.commit_id
  OR NEW.recipe_id IS NOT OLD.recipe_id OR NEW.recipe_version IS NOT OLD.recipe_version
  OR NEW.recipe_instance_id IS NOT OLD.recipe_instance_id OR NEW.counterpart_entity_id IS NOT OLD.counterpart_entity_id
  OR NEW.effective_date IS NOT OLD.effective_date
  OR NEW.description IS NOT OLD.description OR NEW.recorded_at IS NOT OLD.recorded_at
  OR NEW.reverses_id IS NOT OLD.reverses_id
  OR OLD.superseded_by IS NOT NULL
  OR NEW.superseded_by IS NULL
  OR NOT EXISTS (SELECT 1 FROM accounting_events r WHERE r.id = NEW.superseded_by AND r.reverses_id = OLD.id)
  OR EXISTS (SELECT 1 FROM accounting_events r JOIN commits c ON c.id = r.commit_id WHERE r.id = NEW.superseded_by)
BEGIN SELECT RAISE(ABORT, 'ADR 0002 amendment: only superseded_by may be set, once, to the reversing event, inside its commit'); END;

-- ADR 0002: a reversal cancels a current, non-reversal event and is
-- effective-dated to it (corrections are dated to the underlying observation).
-- Correction of a correction reverses the replacement, not the reversal (AT-033).
CREATE TRIGGER accounting_events_reversal_valid BEFORE INSERT ON accounting_events
WHEN NEW.reverses_id IS NOT NULL AND (
       (SELECT reverses_id FROM accounting_events WHERE id = NEW.reverses_id) IS NOT NULL
    OR (SELECT superseded_by FROM accounting_events WHERE id = NEW.reverses_id) IS NOT NULL
    OR (SELECT effective_date FROM accounting_events WHERE id = NEW.reverses_id) IS NOT NEW.effective_date)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: a reversal targets a current non-reversal event and keeps its effective date'); END;
CREATE TRIGGER accounting_events_insert_current BEFORE INSERT ON accounting_events
WHEN NEW.superseded_by IS NOT NULL
BEGIN SELECT RAISE(ABORT, 'ADR 0002 amendment: an event is inserted current; its reversal sets superseded_by'); END;
-- ADR 0004: expectations belong only to a recipe half (so never to a reversal).
CREATE TRIGGER event_counterpart_expectations_recipe_half BEFORE INSERT ON event_counterpart_expectations
WHEN (SELECT recipe_instance_id FROM accounting_events WHERE id = NEW.event_id) IS NULL
BEGIN SELECT RAISE(ABORT, 'ADR 0004: counterpart expectations belong to a recipe half, not a reversal or a plain event'); END;
-- ADR 0004: a recipe half's counterpart is another entity, never this database's.
CREATE TRIGGER accounting_events_counterpart_not_self BEFORE INSERT ON accounting_events
WHEN NEW.counterpart_entity_id IS NOT NULL AND NEW.counterpart_entity_id = (SELECT id FROM entity)
BEGIN SELECT RAISE(ABORT, 'ADR 0004: counterpart_entity_id must be another entity'); END;

-- INV-003 / ADR 0008: income/expense postings use a leaf category.
CREATE TRIGGER journal_postings_category_leaf BEFORE INSERT ON journal_postings
WHEN (SELECT account_type FROM ledger_accounts WHERE id = NEW.ledger_account_id) IN ('income','expense')
 AND EXISTS (SELECT 1 FROM ledger_accounts WHERE parent_id = NEW.ledger_account_id)
BEGIN SELECT RAISE(ABORT, 'INV-003/ADR 0008: income/expense postings must use a leaf category account'); END;
-- INV-005: a posting on a financial-account-backed ledger account is in that account's currency.
CREATE TRIGGER journal_postings_currency_valid BEFORE INSERT ON journal_postings
WHEN coalesce((SELECT f.currency FROM ledger_accounts a JOIN financial_accounts f ON f.id = a.financial_account_id
               WHERE a.id = NEW.ledger_account_id), NEW.currency) IS NOT NEW.currency
BEGIN SELECT RAISE(ABORT, 'INV-005: posting currency must match the financial account currency'); END;

-- INV-015 / ADR 0008: a linked refund is an opposite-side posting to the same
-- income/expense account and currency, and current links never exceed the purchase.
CREATE TRIGGER refund_links_valid BEFORE INSERT ON refund_links
WHEN (SELECT ledger_account_id FROM journal_postings WHERE id = NEW.refund_posting_id) IS NOT (SELECT ledger_account_id FROM journal_postings WHERE id = NEW.purchase_posting_id)
  OR (SELECT currency FROM journal_postings WHERE id = NEW.refund_posting_id) IS NOT (SELECT currency FROM journal_postings WHERE id = NEW.purchase_posting_id)
  OR (SELECT side FROM journal_postings WHERE id = NEW.refund_posting_id) IS (SELECT side FROM journal_postings WHERE id = NEW.purchase_posting_id)
  OR (SELECT a.account_type FROM journal_postings p JOIN ledger_accounts a ON a.id = p.ledger_account_id WHERE p.id = NEW.purchase_posting_id) NOT IN ('income','expense')
BEGIN SELECT RAISE(ABORT, 'ADR 0008: a refund is an opposite-side posting to the purchase''s own category'); END;
CREATE TRIGGER refund_links_cap BEFORE INSERT ON refund_links
WHEN NEW.amount_minor > (SELECT amount_minor FROM journal_postings WHERE id = NEW.refund_posting_id)
  OR NEW.amount_minor + coalesce((
       SELECT sum(l.amount_minor) FROM refund_links l
       JOIN journal_postings rp ON rp.id = l.refund_posting_id
       JOIN accounting_events re ON re.id = rp.event_id
       WHERE l.purchase_posting_id = NEW.purchase_posting_id AND re.superseded_by IS NULL AND re.reverses_id IS NULL), 0)
     > (SELECT amount_minor FROM journal_postings WHERE id = NEW.purchase_posting_id)
BEGIN SELECT RAISE(ABORT, 'INV-015: refund links exceed the refundable purchase amount'); END;

-- --- Observations ------------------------------------------------------------
CREATE TRIGGER observations_currency_valid BEFORE INSERT ON observations
WHEN NEW.financial_account_id IS NOT NULL
 AND (SELECT currency FROM financial_accounts WHERE id = NEW.financial_account_id) IS NOT NEW.currency
BEGIN SELECT RAISE(ABORT, 'INV-005: observation currency must match its financial account'); END;
CREATE TRIGGER observation_revisions_currency_valid BEFORE INSERT ON observation_revisions
WHEN (SELECT f.currency FROM observations o JOIN financial_accounts f ON f.id = o.financial_account_id WHERE o.id = NEW.observation_id) IS NOT NEW.currency
BEGIN SELECT RAISE(ABORT, 'INV-005: observation currency must match its financial account'); END;
CREATE TRIGGER observations_corroborate_valid BEFORE INSERT ON observations
WHEN NEW.corroborates_id IS NOT NULL AND (SELECT financial_account_id FROM observations WHERE id = NEW.corroborates_id) IS NOT NEW.financial_account_id
BEGIN SELECT RAISE(ABORT, 'ADR 0007: a corroborating row must be for the same financial account'); END;
-- INV-007 / ADR 0007: the interpreting posting is on the observation's own
-- financial account, same currency and amount; inflow = debit, outflow = credit
-- (for both asset and liability accounts).
CREATE TRIGGER posting_observations_valid BEFORE INSERT ON posting_observations
WHEN (SELECT a.financial_account_id FROM journal_postings p JOIN ledger_accounts a ON a.id = p.ledger_account_id WHERE p.id = NEW.posting_id)
       IS NOT (SELECT financial_account_id FROM observation_current WHERE id = NEW.observation_id)
  OR (SELECT amount_minor FROM journal_postings WHERE id = NEW.posting_id) IS NOT (SELECT amount_minor FROM observation_current WHERE id = NEW.observation_id)
  OR (SELECT currency FROM journal_postings WHERE id = NEW.posting_id) IS NOT (SELECT currency FROM observation_current WHERE id = NEW.observation_id)
  OR (SELECT side FROM journal_postings WHERE id = NEW.posting_id) IS NOT (SELECT CASE direction WHEN 'inflow' THEN 'debit' ELSE 'credit' END FROM observation_current WHERE id = NEW.observation_id)
BEGIN SELECT RAISE(ABORT, 'INV-007: posting must match its observation (account, amount, currency, direction)'); END;
CREATE TRIGGER posting_observations_not_voided BEFORE INSERT ON posting_observations
WHEN EXISTS (SELECT 1 FROM observation_voids WHERE observation_id = NEW.observation_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0002: a voided observation cannot be interpreted'); END;

-- --- Settlement group state machine (ADR 0006) -------------------------------
CREATE TRIGGER settlement_group_states_transition BEFORE INSERT ON settlement_group_states
WHEN NEW.seq IS NOT coalesce((SELECT max(seq) FROM settlement_group_states WHERE group_id = NEW.group_id), 0) + 1
  OR (NEW.seq = 1 AND NEW.state = 'dissolved')
  OR (SELECT state FROM settlement_group_states WHERE group_id = NEW.group_id AND seq = NEW.seq - 1) = 'dissolved'
  OR ((SELECT state FROM settlement_group_states WHERE group_id = NEW.group_id AND seq = NEW.seq - 1) = 'reconciled' AND NEW.state <> 'dissolved')
BEGIN SELECT RAISE(ABORT, 'ADR 0006: invalid settlement group transition'); END;
CREATE TRIGGER settlement_postings_current BEFORE INSERT ON settlement_postings
WHEN EXISTS (SELECT 1 FROM journal_postings p JOIN accounting_events e ON e.id = p.event_id
             WHERE p.id = NEW.posting_id AND (e.superseded_by IS NOT NULL OR e.reverses_id IS NOT NULL))
BEGIN SELECT RAISE(ABORT, 'ADR 0006: only current, non-reversal postings join a settlement group'); END;

-- --- Obligations (ADR 0001) --------------------------------------------------
CREATE TRIGGER obligations_account_valid BEFORE INSERT ON obligations
WHEN (SELECT kind FROM ledger_accounts WHERE id = NEW.ledger_account_id) IS NOT NEW.direction
BEGIN SELECT RAISE(ABORT, 'ADR 0001: a claim lives in a receivable/payable ledger account matching its direction'); END;
-- ADR 0001 "validator must reject": settlement above the open amount (this also
-- covers forgiving an already-settled claim) and settling a voided (reversed) claim.
CREATE TRIGGER obligation_settlements_cap BEFORE INSERT ON obligation_settlements
WHEN (SELECT e.superseded_by FROM obligations o JOIN accounting_events e ON e.id = o.opened_event_id WHERE o.id = NEW.obligation_id) IS NOT NULL
  OR NEW.amount_minor + coalesce((
       SELECT sum(s.amount_minor) FROM obligation_settlements s JOIN accounting_events e ON e.id = s.event_id
       WHERE s.obligation_id = NEW.obligation_id AND e.superseded_by IS NULL), 0)
     > (SELECT original_minor FROM obligations WHERE id = NEW.obligation_id)
BEGIN SELECT RAISE(ABORT, 'ADR 0001: settlement exceeds the open claim, or the claim is voided'); END;

-- --- Valuation snapshots (ADR 0005) ------------------------------------------
CREATE TRIGGER valuation_snapshots_noncash BEFORE INSERT ON valuation_snapshots
WHEN (SELECT kind FROM financial_accounts WHERE id = NEW.financial_account_id) NOT IN ('brokerage','retirement','hsa','real_estate','vehicle','other_asset')
  OR (SELECT currency FROM financial_accounts WHERE id = NEW.financial_account_id) IS NOT NEW.currency
BEGIN SELECT RAISE(ABORT, 'ADR 0005/INV-011: valuation snapshots only on non-cash asset accounts, in their currency'); END;

-- --- Commit seal: the storage-level commit validator -------------------------
-- Runs when the commits row is inserted, after every other row of the commit.
CREATE TRIGGER commits_seal BEFORE INSERT ON commits
BEGIN
    -- AT-032: optimistic concurrency; seq = expected_revision + 1.
    -- AT-032: the caller's expected_revision must be the current revision.
    SELECT RAISE(ABORT, 'AT-032: stale revision (expected_revision is not the current revision)')
    WHERE NEW.expected_revision IS NOT (SELECT coalesce(max(seq), 0) FROM commits);
    -- ADR 0002: every event in a commit shares the commit's recorded_at.
    SELECT RAISE(ABORT, 'ADR 0002: event recorded_at must equal its commit recorded_at')
    WHERE EXISTS (SELECT 1 FROM accounting_events WHERE commit_id = NEW.id AND recorded_at IS NOT NEW.recorded_at);
    -- ADR 0004: a recipe half carries at least one counterpart expectation.
    SELECT RAISE(ABORT, 'ADR 0004: a recipe half needs at least one counterpart expectation')
    WHERE EXISTS (SELECT 1 FROM accounting_events e WHERE e.commit_id = NEW.id AND e.recipe_instance_id IS NOT NULL
                  AND NOT EXISTS (SELECT 1 FROM event_counterpart_expectations x WHERE x.event_id = e.id));
    -- INV-002: an event has at least two postings.
    SELECT RAISE(ABORT, 'INV-002: every event needs at least two postings')
    WHERE EXISTS (SELECT 1 FROM accounting_events e WHERE e.commit_id = NEW.id
                  AND (SELECT count(*) FROM journal_postings p WHERE p.event_id = e.id) < 2);
    -- INV-001 / ADR 0008: every event balances per currency in journal_postings alone.
    SELECT RAISE(ABORT, 'INV-001: event does not balance per currency')
    WHERE EXISTS (SELECT 1 FROM journal_postings p JOIN accounting_events e ON e.id = p.event_id
                  WHERE e.commit_id = NEW.id
                  GROUP BY p.event_id, p.currency
                  HAVING sum(CASE p.side WHEN 'debit' THEN p.amount_minor ELSE -p.amount_minor END) <> 0);
    -- ADR 0002: a reversal exactly cancels its target, account by account.
    SELECT RAISE(ABORT, 'ADR 0002: reversal does not exactly cancel its target')
    WHERE EXISTS (SELECT 1 FROM accounting_events r JOIN journal_postings p ON p.event_id IN (r.id, r.reverses_id)
                  WHERE r.commit_id = NEW.id AND r.reverses_id IS NOT NULL
                  GROUP BY r.id, p.ledger_account_id, p.currency
                  HAVING sum(CASE p.side WHEN 'debit' THEN p.amount_minor ELSE -p.amount_minor END) <> 0);
    -- ADR 0002 amendment: superseded_by is set in the reversal's own commit.
    SELECT RAISE(ABORT, 'ADR 0002 amendment: a reversed event must be flagged superseded_by in the same commit')
    WHERE EXISTS (SELECT 1 FROM accounting_events r JOIN accounting_events o ON o.id = r.reverses_id
                  WHERE r.commit_id = NEW.id AND o.superseded_by IS NOT r.id);
    -- INV-006: postings join a group only when it reconciles in this commit.
    SELECT RAISE(ABORT, 'INV-006: draft groups never hold postings')
    WHERE EXISTS (SELECT 1 FROM settlement_postings sp WHERE sp.commit_id = NEW.id
                  AND NOT EXISTS (SELECT 1 FROM settlement_group_states s
                                  WHERE s.group_id = sp.group_id AND s.commit_id = NEW.id AND s.state = 'reconciled'));
    SELECT RAISE(ABORT, 'INV-008: a reconciled group must hold its postings')
    WHERE EXISTS (SELECT 1 FROM settlement_group_states s WHERE s.commit_id = NEW.id AND s.state = 'reconciled'
                  AND NOT EXISTS (SELECT 1 FROM settlement_postings sp WHERE sp.group_id = s.group_id));
    -- INV-008 / INV-017 / AT-030: zero residual per group, currency and clearing account.
    SELECT RAISE(ABORT, 'INV-008: settlement group has a nonzero clearing residual')
    WHERE EXISTS (SELECT 1 FROM settlement_group_states s
                  JOIN settlement_postings sp ON sp.group_id = s.group_id
                  JOIN journal_postings p ON p.id = sp.posting_id
                  JOIN ledger_accounts a ON a.id = p.ledger_account_id
                  WHERE s.commit_id = NEW.id AND s.state = 'reconciled' AND a.kind = 'clearing'
                  GROUP BY s.group_id, p.currency, p.ledger_account_id
                  HAVING sum(CASE p.side WHEN 'debit' THEN p.amount_minor ELSE -p.amount_minor END) <> 0);
    -- ADR 0006 / PR 19 Q3: settlement_postings.posting_id is UNIQUE forever, so
    -- dissolving a reconciled group must reverse every leg in the same commit
    -- (or earlier) ...
    SELECT RAISE(ABORT, 'ADR 0006: dissolving a reconciled group must reverse all of its legs')
    WHERE EXISTS (SELECT 1 FROM settlement_group_states s
                  JOIN settlement_group_states prev ON prev.group_id = s.group_id AND prev.seq = s.seq - 1 AND prev.state = 'reconciled'
                  JOIN settlement_postings sp ON sp.group_id = s.group_id
                  JOIN journal_postings p ON p.id = sp.posting_id
                  JOIN accounting_events e ON e.id = p.event_id
                  WHERE s.commit_id = NEW.id AND s.state = 'dissolved' AND e.superseded_by IS NULL);
    -- ... and repost every observed leg that still needs reconciling: each
    -- non-voided observation behind a reversed leg has a current interpreting
    -- posting after the commit. (Re-grouping the replacement: U19.)
    SELECT RAISE(ABORT, 'ADR 0006: a dissolved group''s observed legs must be reposted in the same commit')
    WHERE EXISTS (SELECT 1 FROM settlement_group_states s
                  JOIN settlement_group_states prev ON prev.group_id = s.group_id AND prev.seq = s.seq - 1 AND prev.state = 'reconciled'
                  JOIN settlement_postings sp ON sp.group_id = s.group_id
                  JOIN posting_observations po ON po.posting_id = sp.posting_id
                  WHERE s.commit_id = NEW.id AND s.state = 'dissolved'
                    AND NOT EXISTS (SELECT 1 FROM observation_voids v WHERE v.observation_id = po.observation_id)
                    AND NOT EXISTS (SELECT 1 FROM posting_observations po2
                                    JOIN journal_postings p2 ON p2.id = po2.posting_id
                                    JOIN accounting_events e2 ON e2.id = p2.event_id
                                    WHERE po2.observation_id = po.observation_id
                                      AND e2.superseded_by IS NULL AND e2.reverses_id IS NULL));
END;

-- =============================================================================
-- VIEWS (read models; rebuildable, never authoritative)
-- =============================================================================
-- ADR 0002 amendment: balances are the plain sum of ALL postings (reversals net
-- out), never filtered by superseded_by. Signed in the account's normal
-- direction (debit-normal: asset, expense; credit-normal: the rest).
CREATE VIEW account_balances AS
SELECT a.id AS ledger_account_id, a.account_type, a.kind, p.currency,
       sum(CASE WHEN (p.side = 'debit') = (a.account_type IN ('asset','expense')) THEN p.amount_minor ELSE -p.amount_minor END) AS balance_minor
FROM journal_postings p JOIN ledger_accounts a ON a.id = p.ledger_account_id
GROUP BY a.id, p.currency;

-- ADR 0008 / AT-034: trial balance from journal_postings alone. Per currency
-- the net_debit_minor rows sum to zero: A = L + Eq + (I - X).
CREATE VIEW trial_balance AS
SELECT p.currency, a.account_type,
       sum(CASE p.side WHEN 'debit' THEN p.amount_minor ELSE -p.amount_minor END) AS net_debit_minor
FROM journal_postings p JOIN ledger_accounts a ON a.id = p.ledger_account_id
GROUP BY p.currency, a.account_type;

-- ADR 0002 amendment: the register hides superseded entries and their reversals
-- (served by the accounting_events_register partial index); audit reads the table.
CREATE VIEW register_events AS
SELECT * FROM accounting_events WHERE superseded_by IS NULL AND reverses_id IS NULL;

-- ADR 0006: a group's state is its latest state row.
CREATE VIEW settlement_group_current AS
SELECT s.group_id, s.state, s.seq, s.commit_id
FROM settlement_group_states s
WHERE s.seq = (SELECT max(seq) FROM settlement_group_states x WHERE x.group_id = s.group_id);

-- ADR 0006 / GLOSSARY "Review state": derived per live observation, never stored.
-- A voided observation is off this view. Origin (observed or expected) is separate.
CREATE VIEW observation_review_state AS
SELECT o.id AS observation_id,
       CASE
         WHEN EXISTS (SELECT 1 FROM posting_observations po
                      JOIN journal_postings p ON p.id = po.posting_id
                      JOIN accounting_events e ON e.id = p.event_id
                      JOIN settlement_postings sp ON sp.posting_id = p.id
                      JOIN settlement_group_current g ON g.group_id = sp.group_id
                      WHERE po.observation_id = o.id AND e.superseded_by IS NULL AND e.reverses_id IS NULL
                        AND g.state = 'reconciled') THEN 'reconciled'
         WHEN EXISTS (SELECT 1 FROM settlement_draft_participants dp
                      JOIN settlement_group_states s ON s.id = dp.group_state_id
                      JOIN settlement_group_current g ON g.group_id = s.group_id AND g.seq = s.seq
                      WHERE dp.observation_id = o.id AND g.state = 'draft') THEN 'interpreted'
         ELSE 'unreviewed'
       END AS review_state
FROM observations o
WHERE o.corroborates_id IS NULL
  AND NOT EXISTS (SELECT 1 FROM observation_voids v WHERE v.observation_id = o.id);

-- GLOSSARY "Origin": observed while any live observation link points at the line,
-- otherwise expected. A link is live when its observation has no void tombstone.
CREATE VIEW posting_origin AS
SELECT p.id AS posting_id,
       CASE WHEN EXISTS (
         SELECT 1 FROM posting_observations po
         WHERE po.posting_id = p.id
           AND NOT EXISTS (
             SELECT 1 FROM observation_voids v WHERE v.observation_id = po.observation_id
           )
       ) THEN 'observed' ELSE 'expected' END AS origin
FROM journal_postings p;

-- ADR 0001: remaining claim amount, derived.
CREATE VIEW obligation_balances AS
SELECT o.id AS obligation_id, o.currency, o.original_minor,
       o.original_minor - coalesce((SELECT sum(s.amount_minor) FROM obligation_settlements s
                                    JOIN accounting_events e ON e.id = s.event_id
                                    WHERE s.obligation_id = o.id AND e.superseded_by IS NULL), 0) AS remaining_minor
FROM obligations o;

-- PR 19 Q5: current display name = latest rename, else the original name.
CREATE VIEW reference_current_names AS
SELECT r.target_kind, r.target_id, r.name FROM reference_renames r
WHERE r.recorded_at || r.id = (SELECT max(x.recorded_at || x.id) FROM reference_renames x
                               WHERE x.target_kind = r.target_kind AND x.target_id = r.target_id);

-- doctor (U21): each view must return no rows. doctor also runs
-- PRAGMA integrity_check and PRAGMA foreign_key_check (SPEC-v2-delta §3).
-- PR 19 fix 1(b): rows in commit-scoped tables (and rows hanging off them)
-- whose commit has no commits row. Only possible on a connection that wrote
-- with foreign_keys OFF; such rows skipped every seal check.
CREATE VIEW doctor_unsealed_rows AS
SELECT 'user_grants' AS table_name, t.rowid AS row_id, t.commit_id FROM user_grants t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'accounting_events' AS table_name, t.rowid AS row_id, t.commit_id FROM accounting_events t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'evidence_deletions' AS table_name, t.rowid AS row_id, t.commit_id FROM evidence_deletions t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'evidence_links' AS table_name, t.rowid AS row_id, t.commit_id FROM evidence_links t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'import_runs' AS table_name, t.rowid AS row_id, t.commit_id FROM import_runs t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'observations' AS table_name, t.rowid AS row_id, t.commit_id FROM observations t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'observation_revisions' AS table_name, t.rowid AS row_id, t.commit_id FROM observation_revisions t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'observation_voids' AS table_name, t.rowid AS row_id, t.commit_id FROM observation_voids t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'settlement_groups' AS table_name, t.rowid AS row_id, t.commit_id FROM settlement_groups t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'settlement_group_states' AS table_name, t.rowid AS row_id, t.commit_id FROM settlement_group_states t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'settlement_postings' AS table_name, t.rowid AS row_id, t.commit_id FROM settlement_postings t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'obligations' AS table_name, t.rowid AS row_id, t.commit_id FROM obligations t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'obligation_settlements' AS table_name, t.rowid AS row_id, t.commit_id FROM obligation_settlements t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'statements' AS table_name, t.rowid AS row_id, t.commit_id FROM statements t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'balance_assertions' AS table_name, t.rowid AS row_id, t.commit_id FROM balance_assertions t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'valuation_snapshots' AS table_name, t.rowid AS row_id, t.commit_id FROM valuation_snapshots t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'kernel_state' AS table_name, t.rowid AS row_id, t.commit_id FROM kernel_state t WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = t.commit_id)
UNION ALL SELECT 'journal_postings', t.rowid, e.commit_id FROM journal_postings t LEFT JOIN accounting_events e ON e.id = t.event_id WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = e.commit_id)
UNION ALL SELECT 'posting_tags', t.rowid, e.commit_id FROM posting_tags t LEFT JOIN journal_postings p ON p.id = t.posting_id LEFT JOIN accounting_events e ON e.id = p.event_id WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = e.commit_id)
UNION ALL SELECT 'event_counterpart_expectations', t.rowid, e.commit_id FROM event_counterpart_expectations t LEFT JOIN accounting_events e ON e.id = t.event_id WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = e.commit_id)
UNION ALL SELECT 'posting_observations', t.rowid, e.commit_id FROM posting_observations t LEFT JOIN journal_postings p ON p.id = t.posting_id LEFT JOIN accounting_events e ON e.id = p.event_id WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = e.commit_id)
UNION ALL SELECT 'refund_links', t.rowid, e.commit_id FROM refund_links t LEFT JOIN journal_postings p ON p.id = t.refund_posting_id LEFT JOIN accounting_events e ON e.id = p.event_id WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = e.commit_id)
UNION ALL SELECT 'obligation_postings', t.rowid, o.commit_id FROM obligation_postings t LEFT JOIN obligations o ON o.id = t.obligation_id WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = o.commit_id)
UNION ALL SELECT 'settlement_draft_participants', t.rowid, s.commit_id FROM settlement_draft_participants t LEFT JOIN settlement_group_states s ON s.id = t.group_state_id WHERE NOT EXISTS (SELECT 1 FROM commits c WHERE c.id = s.commit_id);
CREATE VIEW doctor_unbalanced_events AS            -- INV-001, per currency
SELECT event_id, currency FROM journal_postings
GROUP BY event_id, currency
HAVING sum(CASE side WHEN 'debit' THEN amount_minor ELSE -amount_minor END) <> 0;
CREATE VIEW doctor_superseded_drift AS             -- ADR 0002 amendment: flag vs reverses_id
SELECT e.id FROM accounting_events e LEFT JOIN accounting_events r ON r.reverses_id = e.id
WHERE e.superseded_by IS NOT r.id;
CREATE VIEW doctor_observation_double_consumed AS  -- INV-007 among live current links
SELECT po.observation_id FROM posting_observations po
JOIN journal_postings p ON p.id = po.posting_id JOIN accounting_events e ON e.id = p.event_id
WHERE e.superseded_by IS NULL AND e.reverses_id IS NULL
  AND NOT EXISTS (SELECT 1 FROM observation_voids v WHERE v.observation_id = po.observation_id)
GROUP BY po.observation_id HAVING count(*) > 1;

-- =============================================================================
-- NOT ENFORCED HERE (commit validator U19 / doctor U21 / application)
-- =============================================================================
-- * ADR 0002 closed-month invariant: every balance assertion with cutoff on or
--   after the commit's earliest effective_date (for a correction:
--   min(reversal date, replacement date)), on every account touched,
--   recomputes unchanged (needs SPEC §6 arithmetic; U19 with U32).
-- * Re-grouping a dissolved group's reposted legs (U19); storage only checks
--   that they are reversed and, if observed, reinterpreted.
-- * PRAGMA foreign_keys = 1 on every writing connection (kernel open(), §3).
-- * INV-013 transitive validation of corrections (groups, obligations, refunds).
-- * INV-008 group completeness under its recipe (required roles present).
-- * INV-010 statement reconciliation (exact agreement after in-flight items).
-- * ADR 0004 cross-database pairing of recipe halves (application recipe check);
--   evidence hash checks (doctor).
-- * INV-016/018/019 reporting and MCP rules (MCP tables deferred past month 1).
-- * Reporting groups / consolidation span databases and live in application config.
-- * Account ownership transfer between entities (v0.2 open decision 7, deferred).
