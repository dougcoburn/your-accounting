-- Initial draft; domain-service validation still required for balancing, group finalization and transitive corrections.
-- Amended 2026-11-22 (file name kept): ADR 0008 pure double entry (categories are income/expense ledger accounts;
-- allocations table removed) and ADR 0002 amendment (recorded_at, reverses_id, superseded_by currency flag).
-- Kernel balance rule (commit validator, ADR 0008): for every accounting event, sum(debit) = sum(credit) per
-- (entity, currency) in journal_postings alone; the trial balance and A = L + Eq + (Income - Expense) close from this one table.

PRAGMA foreign_keys=ON;
CREATE TABLE entities(id TEXT PRIMARY KEY,name TEXT NOT NULL,visibility TEXT NOT NULL CHECK(visibility IN ('transparent','opaque')));
CREATE TABLE financial_accounts(id TEXT PRIMARY KEY,owner_entity_id TEXT NOT NULL REFERENCES entities(id),name TEXT NOT NULL,kind TEXT NOT NULL,currency TEXT NOT NULL CHECK(length(currency)=3));
-- account_type = GAAP element; kind = engine sub-kind (e.g. cash, card, receivable, payable, clearing, category, opening_equity).
-- Categories are income/expense ledger accounts: per entity, hierarchical via parent_id, postings only to leaves.
-- Contra amounts (refunds, card-perk credits, discounts) are opposite-side postings to the original income/expense account; no contra table.
CREATE TABLE ledger_accounts(id TEXT PRIMARY KEY,entity_id TEXT NOT NULL REFERENCES entities(id),financial_account_id TEXT REFERENCES financial_accounts(id),account_type TEXT NOT NULL CHECK(account_type IN ('asset','liability','equity','income','expense')),kind TEXT NOT NULL,parent_id TEXT REFERENCES ledger_accounts(id),name TEXT NOT NULL,CHECK(parent_id IS NULL OR account_type IN ('income','expense')));
CREATE INDEX ledger_accounts_children ON ledger_accounts(parent_id) WHERE parent_id IS NOT NULL;
CREATE TABLE parties(id TEXT PRIMARY KEY,name TEXT NOT NULL,opaque_entity_id TEXT REFERENCES entities(id));
CREATE TABLE canonical_transactions(id TEXT PRIMARY KEY,party_id TEXT REFERENCES parties(id),description TEXT,state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','validated','reconciled')),revision INTEGER NOT NULL DEFAULT 0);
CREATE TABLE account_movements(id TEXT PRIMARY KEY,transaction_id TEXT NOT NULL REFERENCES canonical_transactions(id),financial_account_id TEXT NOT NULL REFERENCES financial_accounts(id),posting_date TEXT NOT NULL,direction TEXT NOT NULL CHECK(direction IN ('increase','decrease')),amount_minor INTEGER NOT NULL CHECK(amount_minor>0),verified INTEGER NOT NULL DEFAULT 0 CHECK(verified IN (0,1)));
CREATE INDEX account_movements_lookup ON account_movements(financial_account_id,posting_date);
CREATE TABLE scenarios(id TEXT NOT NULL,version TEXT NOT NULL,PRIMARY KEY(id,version));
-- Append-only (ADR 0002). superseded_by is the only column ever set after insert: NULL -> id of the event that reverses this one,
-- written in the same commit as that reversing event. Doctor rebuilds it from reverses_id. Balances ignore it (reversals net out).
CREATE TABLE accounting_events(id TEXT PRIMARY KEY,scenario_id TEXT NOT NULL,scenario_version TEXT NOT NULL,posting_date TEXT NOT NULL,effective_date_override TEXT,recorded_at TEXT NOT NULL,reverses_id TEXT REFERENCES accounting_events(id),superseded_by TEXT REFERENCES accounting_events(id),FOREIGN KEY(scenario_id,scenario_version) REFERENCES scenarios(id,version));
CREATE UNIQUE INDEX accounting_events_reverses ON accounting_events(reverses_id) WHERE reverses_id IS NOT NULL;
CREATE INDEX accounting_events_register ON accounting_events(posting_date) WHERE superseded_by IS NULL AND reverses_id IS NULL;
CREATE TABLE event_transactions(event_id TEXT NOT NULL REFERENCES accounting_events(id),transaction_id TEXT NOT NULL REFERENCES canonical_transactions(id),PRIMARY KEY(event_id,transaction_id));
CREATE TABLE journal_postings(id TEXT PRIMARY KEY,event_id TEXT NOT NULL REFERENCES accounting_events(id),ledger_account_id TEXT NOT NULL REFERENCES ledger_accounts(id),entity_id TEXT NOT NULL REFERENCES entities(id),currency TEXT NOT NULL,side TEXT NOT NULL CHECK(side IN ('debit','credit')),amount_minor INTEGER NOT NULL CHECK(amount_minor>0));
CREATE INDEX journal_event_balance ON journal_postings(event_id,entity_id,currency,side);
CREATE INDEX journal_postings_account ON journal_postings(ledger_account_id,event_id);
CREATE TABLE settlement_groups(id TEXT PRIMARY KEY,state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','validated','reconciled')),revision INTEGER NOT NULL DEFAULT 0);
CREATE TABLE settlement_participants(id TEXT PRIMARY KEY,group_id TEXT NOT NULL REFERENCES settlement_groups(id),transaction_id TEXT REFERENCES canonical_transactions(id),event_id TEXT REFERENCES accounting_events(id),expected_json TEXT,CHECK(transaction_id IS NOT NULL OR event_id IS NOT NULL OR expected_json IS NOT NULL));
CREATE TABLE settlement_postings(group_id TEXT NOT NULL REFERENCES settlement_groups(id),posting_id TEXT NOT NULL UNIQUE REFERENCES journal_postings(id),PRIMARY KEY(group_id,posting_id));
CREATE TABLE obligations(id TEXT PRIMARY KEY,creditor_entity_id TEXT REFERENCES entities(id),debtor_entity_id TEXT REFERENCES entities(id),creditor_party_id TEXT REFERENCES parties(id),debtor_party_id TEXT REFERENCES parties(id),currency TEXT NOT NULL,original_minor INTEGER NOT NULL CHECK(original_minor>0));
CREATE TABLE obligation_settlements(id TEXT PRIMARY KEY,obligation_id TEXT NOT NULL REFERENCES obligations(id),event_id TEXT NOT NULL REFERENCES accounting_events(id),amount_minor INTEGER NOT NULL CHECK(amount_minor>0));
CREATE TABLE evidence_objects(sha256 TEXT PRIMARY KEY CHECK(length(sha256)=64),size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),status TEXT NOT NULL CHECK(status IN ('present','deleted')));
CREATE TABLE evidence_documents(id TEXT PRIMARY KEY,sha256 TEXT NOT NULL REFERENCES evidence_objects(sha256),filename TEXT,created_at TEXT NOT NULL);
CREATE TABLE extraction_runs(id TEXT PRIMARY KEY,document_id TEXT NOT NULL REFERENCES evidence_documents(id),importer_id TEXT NOT NULL,extracted_at TEXT NOT NULL);
CREATE TABLE observations(id TEXT PRIMARY KEY,document_id TEXT NOT NULL REFERENCES evidence_documents(id),extraction_run_id TEXT REFERENCES extraction_runs(id),payload_json TEXT NOT NULL,accepted_at TEXT);
CREATE TABLE movement_observations(movement_id TEXT NOT NULL REFERENCES account_movements(id),observation_id TEXT NOT NULL UNIQUE REFERENCES observations(id),PRIMARY KEY(movement_id,observation_id));
CREATE TABLE statements(id TEXT PRIMARY KEY,account_id TEXT NOT NULL REFERENCES financial_accounts(id),end_date TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'imported' CHECK(state IN ('imported','reviewed','reconciled')));
CREATE TABLE balance_assertions(id TEXT PRIMARY KEY,account_id TEXT NOT NULL REFERENCES financial_accounts(id),statement_id TEXT REFERENCES statements(id),cutoff TEXT NOT NULL,balance_minor INTEGER NOT NULL,source TEXT NOT NULL CHECK(source IN ('manual','statement')));
CREATE TABLE evidence_links(document_id TEXT NOT NULL REFERENCES evidence_documents(id),transaction_id TEXT REFERENCES canonical_transactions(id),event_id TEXT REFERENCES accounting_events(id),CHECK((transaction_id IS NOT NULL)!=(event_id IS NOT NULL)));
CREATE TABLE audit_operations(id TEXT PRIMARY KEY,actor TEXT NOT NULL,reason TEXT NOT NULL,occurred_at TEXT NOT NULL);
CREATE TABLE audit_changes(id TEXT PRIMARY KEY,operation_id TEXT NOT NULL REFERENCES audit_operations(id),record_type TEXT NOT NULL,record_id TEXT NOT NULL,before_json TEXT,after_json TEXT);
CREATE TABLE tags(id TEXT PRIMARY KEY,name TEXT NOT NULL UNIQUE);
CREATE TABLE transaction_tags(transaction_id TEXT NOT NULL REFERENCES canonical_transactions(id),tag_id TEXT NOT NULL REFERENCES tags(id),PRIMARY KEY(transaction_id,tag_id));
CREATE TABLE posting_tags(posting_id TEXT NOT NULL REFERENCES journal_postings(id),tag_id TEXT NOT NULL REFERENCES tags(id),PRIMARY KEY(posting_id,tag_id));
CREATE TABLE refund_links(id TEXT PRIMARY KEY,refund_posting_id TEXT NOT NULL REFERENCES journal_postings(id),purchase_posting_id TEXT NOT NULL REFERENCES journal_postings(id),amount_minor INTEGER NOT NULL CHECK(amount_minor>0));
CREATE TABLE reporting_groups(id TEXT PRIMARY KEY,name TEXT NOT NULL);
CREATE TABLE reporting_group_members(group_id TEXT NOT NULL REFERENCES reporting_groups(id),entity_id TEXT NOT NULL REFERENCES entities(id),PRIMARY KEY(group_id,entity_id));
CREATE TABLE mcp_clients(id TEXT PRIMARY KEY,name TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('active','revoked')));
CREATE TABLE mcp_grants(id TEXT PRIMARY KEY,client_id TEXT NOT NULL REFERENCES mcp_clients(id),scope_json TEXT NOT NULL,lifetime TEXT NOT NULL CHECK(lifetime IN ('one_operation','session','persistent')),expires_at TEXT,revoked_at TEXT);
CREATE TABLE mcp_permission_requests(id TEXT PRIMARY KEY,client_id TEXT NOT NULL REFERENCES mcp_clients(id),scope_json TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('pending','approved','denied')));
CREATE TABLE mcp_access_audit(id TEXT PRIMARY KEY,client_id TEXT NOT NULL REFERENCES mcp_clients(id),operation TEXT NOT NULL,occurred_at TEXT NOT NULL,allowed INTEGER NOT NULL CHECK(allowed IN (0,1)));
CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL);
CREATE TRIGGER account_owner_valid BEFORE INSERT ON financial_accounts WHEN (SELECT visibility FROM entities WHERE id=NEW.owner_entity_id)!='transparent' BEGIN SELECT RAISE(ABORT,'account owner must be transparent'); END;
CREATE TRIGGER posting_entity_valid BEFORE INSERT ON journal_postings WHEN (SELECT entity_id FROM ledger_accounts WHERE id=NEW.ledger_account_id)!=NEW.entity_id BEGIN SELECT RAISE(ABORT,'ledger entity mismatch'); END;
CREATE TRIGGER posting_category_leaf BEFORE INSERT ON journal_postings WHEN (SELECT account_type FROM ledger_accounts WHERE id=NEW.ledger_account_id) IN ('income','expense') AND EXISTS(SELECT 1 FROM ledger_accounts WHERE parent_id=NEW.ledger_account_id) BEGIN SELECT RAISE(ABORT,'income/expense postings must use a leaf category account'); END;
CREATE TRIGGER category_parent_valid BEFORE INSERT ON ledger_accounts WHEN NEW.parent_id IS NOT NULL AND ((SELECT entity_id FROM ledger_accounts WHERE id=NEW.parent_id)!=NEW.entity_id OR (SELECT account_type FROM ledger_accounts WHERE id=NEW.parent_id)!=NEW.account_type OR EXISTS(SELECT 1 FROM journal_postings WHERE ledger_account_id=NEW.parent_id)) BEGIN SELECT RAISE(ABORT,'category parent must be a same-entity, same-type account with no postings'); END;
CREATE TRIGGER journal_postings_no_update BEFORE UPDATE ON journal_postings BEGIN SELECT RAISE(ABORT,'append-only: postings are never updated'); END;
CREATE TRIGGER journal_postings_no_delete BEFORE DELETE ON journal_postings BEGIN SELECT RAISE(ABORT,'append-only: postings are never deleted'); END;
CREATE TRIGGER accounting_events_no_delete BEFORE DELETE ON accounting_events BEGIN SELECT RAISE(ABORT,'append-only: events are never deleted'); END;
CREATE TRIGGER accounting_events_only_supersede BEFORE UPDATE ON accounting_events WHEN NEW.id IS NOT OLD.id OR NEW.scenario_id IS NOT OLD.scenario_id OR NEW.scenario_version IS NOT OLD.scenario_version OR NEW.posting_date IS NOT OLD.posting_date OR NEW.effective_date_override IS NOT OLD.effective_date_override OR NEW.recorded_at IS NOT OLD.recorded_at OR NEW.reverses_id IS NOT OLD.reverses_id OR OLD.superseded_by IS NOT NULL OR NEW.superseded_by IS NULL OR NOT EXISTS(SELECT 1 FROM accounting_events WHERE id=NEW.superseded_by AND reverses_id=OLD.id) BEGIN SELECT RAISE(ABORT,'append-only: only superseded_by may be set, once, to the event that reverses this one'); END;
-- Trial balance from journal_postings alone (ADR 0008). Every (entity, currency) row nets to zero.
CREATE VIEW trial_balance AS SELECT p.entity_id,p.currency,a.account_type,SUM(CASE p.side WHEN 'debit' THEN p.amount_minor ELSE -p.amount_minor END) AS net_debit_minor FROM journal_postings p JOIN ledger_accounts a ON a.id=p.ledger_account_id GROUP BY p.entity_id,p.currency,a.account_type;
-- Doctor checks (must return no rows): unbalanced events; superseded_by flag drifting from reversal links.
CREATE VIEW doctor_unbalanced_events AS SELECT event_id,entity_id,currency FROM journal_postings GROUP BY event_id,entity_id,currency HAVING SUM(CASE side WHEN 'debit' THEN amount_minor ELSE -amount_minor END)!=0;
CREATE VIEW doctor_superseded_drift AS SELECT e.id FROM accounting_events e LEFT JOIN accounting_events r ON r.reverses_id=e.id WHERE e.superseded_by IS NOT r.id;
