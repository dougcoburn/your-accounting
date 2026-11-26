#!/usr/bin/env bash
# Smoke checks for docs/schema-v2.sql. SYNTHETIC data only (invented ids,
# accounts and amounts); never point this at a real database.
# Usage: docs/schema-v2-smoke.sh   (needs the sqlite3 CLI, >= 3.37)
set -euo pipefail
cd "$(dirname "$0")/.."
DB="$(mktemp -d)/smoke.sqlite"
trap 'rm -rf "$(dirname "$DB")"' EXIT
pass=0; fail=0
run() { sqlite3 -bail "$DB" "PRAGMA foreign_keys=ON;" "$@" 2>&1; }
ok() {   # ok NAME SQL
  if out=$(run "$2"); then echo "PASS  $1"; pass=$((pass+1)); else echo "FAIL  $1: $out"; fail=$((fail+1)); fi
}
rejects() {   # rejects NAME EXPECTED-ERROR-SUBSTRING SQL
  if out=$(run "$3"); then echo "FAIL  $1: was accepted"; fail=$((fail+1));
  elif grep -qF -- "$2" <<<"$out"; then echo "PASS  $1 -> ${out##*: }"; pass=$((pass+1));
  else echo "FAIL  $1: wrong error: $out"; fail=$((fail+1)); fi
}
equals() {   # equals NAME EXPECTED SQL
  out=$(run "$3" || true)
  if [[ "$out" == "$2" ]]; then echo "PASS  $1"; pass=$((pass+1)); else echo "FAIL  $1: got [$out] want [$2]"; fail=$((fail+1)); fi
}

sqlite3 -bail "$DB" < docs/schema-v2.sql && echo "PASS  schema loads (user_version $(sqlite3 "$DB" 'PRAGMA user_version'))"

# Synthetic ULIDs: 22-char prefix + 4-char suffix (Crockford base32).
P=01J0000000000000000000
T=2026-01-02T03:04:05Z

rejects "ids use Crockford base32 (L, O, I, U are invalid)" "CHECK constraint failed" "INSERT INTO parties(id,name,created_at) VALUES('${P}LOIU','Bad id','$T');"
ok "setup: entity, accounts, category tree (synthetic)" "
BEGIN;
INSERT INTO entity(id,party_id,name,created_at) VALUES('${P}ENT1','${P}PTY1','Synthetic Household','$T');
INSERT INTO parties(id,name,label,created_at) VALUES('${P}SHP1','Example Grocer','vendor','$T');
INSERT INTO recipes VALUES('card_purchase','1.0.0'),('correction','1.0.0');
INSERT INTO feeds VALUES('${P}FED1','monarch_csv','Synthetic feed');
INSERT INTO financial_accounts(id,name,kind,currency,tracking_mode,last4,created_at) VALUES('${P}FAC1','Card','credit_card','USD','transparent','0000','$T');
INSERT INTO ledger_accounts(id,entity_id,financial_account_id,account_type,kind,name,created_at) VALUES('${P}AC01','${P}ENT1','${P}FAC1','liability','card','Card','$T');
INSERT INTO ledger_accounts(id,entity_id,account_type,kind,name,created_at) VALUES('${P}AX10','${P}ENT1','expense','category','Food','$T');
INSERT INTO ledger_accounts(id,entity_id,account_type,kind,parent_id,name,created_at) VALUES('${P}AX11','${P}ENT1','expense','category','${P}AX10','Groceries','$T');
INSERT INTO ledger_accounts(id,entity_id,account_type,kind,parent_id,name,created_at) VALUES('${P}AX12','${P}ENT1','expense','category','${P}AX10','Dining','$T');
INSERT INTO ledger_accounts(id,entity_id,account_type,kind,name,created_at) VALUES('${P}AR01','${P}ENT1','asset','receivable','Claims receivable','$T');
INSERT INTO financial_accounts(id,name,kind,currency,tracking_mode,created_at) VALUES('${P}FAB1','Brokerage','brokerage','USD','transparent','$T');
INSERT INTO tags(id,name) VALUES('${P}TG01','synthetic-tag');
INSERT INTO evidence_objects(sha256,size_bytes,recorded_at) VALUES('$(printf 'b%.0s' {1..64})',10,'$T');
INSERT INTO evidence_documents(id,sha256,recorded_at) VALUES('${P}D0C1','$(printf 'b%.0s' {1..64})','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C001',0,1,'setup','smoke','$T');
COMMIT;"

ok "commit 2: import + card purchase reconciled, plus a synthetic claim, in one commit" "
BEGIN;
INSERT INTO observations(id,feed_id,external_id,financial_account_id,occurred_date,posted_date,direction,amount_minor,currency,row_hash,payload_json,commit_id,recorded_at)
  VALUES('${P}B001','${P}FED1','synthetic-1','${P}FAC1','2025-10-30','2025-11-01','outflow',4250,'USD','$(printf 'a%.0s' {1..64})','{\"synthetic\":true}','${P}C002','$T');
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV01','${P}C002','card_purchase','1.0.0','2025-10-30','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor,party_id) VALUES('${P}PS01','${P}EV01','${P}AX11','${P}ENT1','USD','debit',4250,'${P}SHP1');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS02','${P}EV01','${P}AC01','${P}ENT1','USD','credit',4250);
INSERT INTO posting_observations VALUES('${P}PS02','${P}B001');
INSERT INTO settlement_groups(id,recipe_id,recipe_version,commit_id,recorded_at) VALUES('${P}GRP1','card_purchase','1.0.0','${P}C002','$T');
INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS11','${P}GRP1',1,'reconciled','${P}C002','$T');
INSERT INTO settlement_postings VALUES('${P}GRP1','${P}PS02','${P}C002');
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV0A','${P}C002','card_purchase','1.0.0','2025-10-30','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS0A','${P}EV0A','${P}AR01','${P}ENT1','USD','debit',1000);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS0B','${P}EV0A','${P}AC01','${P}ENT1','USD','credit',1000);
INSERT INTO obligations(id,party_id,direction,ledger_account_id,currency,original_minor,opened_event_id,commit_id,recorded_at) VALUES('${P}0B01','${P}SHP1','receivable','${P}AR01','USD',1000,'${P}EV0A','${P}C002','$T');
INSERT INTO obligation_postings VALUES('${P}0B01','${P}PS0A');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C002',1,2,'interpret','smoke','$T');
COMMIT;"

equals "review state derived: reconciled" "reconciled" "SELECT review_state FROM observation_review_state WHERE observation_id='${P}B001';"

# --- append-only ---------------------------------------------------------------
rejects "UPDATE of an event fact is rejected" "only superseded_by may be set" "UPDATE accounting_events SET description='edited' WHERE id='${P}EV01';"
rejects "UPDATE of effective_date is rejected" "only superseded_by may be set" "UPDATE accounting_events SET effective_date='2025-10-31' WHERE id='${P}EV01';"
rejects "DELETE of an event is rejected" "accounting_events rows are never deleted" "DELETE FROM accounting_events WHERE id='${P}EV01';"
rejects "UPDATE of a posting is rejected" "never updated" "UPDATE journal_postings SET amount_minor=1 WHERE id='${P}PS01';"
rejects "DELETE of a posting is rejected" "never deleted" "DELETE FROM journal_postings WHERE id='${P}PS01';"
rejects "DELETE of an observation is rejected (void instead)" "void an observation" "DELETE FROM observations WHERE id='${P}B001';"
rejects "superseded_by without a reversal is rejected" "only superseded_by may be set" "UPDATE accounting_events SET superseded_by='${P}EV01' WHERE id='${P}EV01';"
rejects "posting added to a sealed event is rejected" "open commit" "INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PSX1','${P}EV01','${P}AX11','${P}ENT1','USD','debit',1);"

# --- correction: reversal + replacement, superseded_by set in the same commit ---
ok "commit 3: recategorize (reversal + replacement, flag set once, regroup)" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at,reverses_id) VALUES('${P}EV02','${P}C003','correction','1.0.0','2025-10-30','$T','${P}EV01');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS03','${P}EV02','${P}AX11','${P}ENT1','USD','credit',4250);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS04','${P}EV02','${P}AC01','${P}ENT1','USD','debit',4250);
UPDATE accounting_events SET superseded_by='${P}EV02' WHERE id='${P}EV01';
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV03','${P}C003','card_purchase','1.0.0','2025-10-30','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS05','${P}EV03','${P}AX12','${P}ENT1','USD','debit',4250);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS06','${P}EV03','${P}AC01','${P}ENT1','USD','credit',4250);
INSERT INTO posting_observations VALUES('${P}PS06','${P}B001');
INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS12','${P}GRP1',2,'dissolved','${P}C003','$T');
INSERT INTO settlement_groups(id,recipe_id,recipe_version,commit_id,recorded_at) VALUES('${P}GRP2','card_purchase','1.0.0','${P}C003','$T');
INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS21','${P}GRP2',1,'reconciled','${P}C003','$T');
INSERT INTO settlement_postings VALUES('${P}GRP2','${P}PS06','${P}C003');
INSERT INTO commits(id,expected_revision,seq,kind,actor,reason,recorded_at) VALUES('${P}C003',2,3,'correct','smoke','recategorize','$T');
COMMIT;"

rejects "superseded_by cannot be set a second time" "only superseded_by may be set" "UPDATE accounting_events SET superseded_by='${P}EV03' WHERE id='${P}EV01';"
rejects "superseded_by cannot be cleared" "only superseded_by may be set" "UPDATE accounting_events SET superseded_by=NULL WHERE id='${P}EV01';"
rejects "superseded_by cannot be set after the reversal's commit sealed" "only superseded_by may be set" "UPDATE accounting_events SET superseded_by='${P}EV02' WHERE id='${P}EV01';"
rejects "a reversal must flag its target in the same commit" "must be flagged superseded_by" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at,reverses_id) VALUES('${P}EV04','${P}C004','correction','1.0.0','2025-10-30','$T','${P}EV03');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS07','${P}EV04','${P}AX12','${P}ENT1','USD','credit',4250);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS08','${P}EV04','${P}AC01','${P}ENT1','USD','debit',4250);
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C004',3,4,'correct','smoke','$T');
COMMIT;"
rejects "a reversal of a reversal is rejected (AT-033 reverses the replacement)" "a reversal targets a current non-reversal event" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at,reverses_id) VALUES('${P}EV05','${P}C004','correction','1.0.0','2025-10-30','$T','${P}EV02');
COMMIT;"
rejects "a reversal must keep its target's effective date" "keeps its effective date" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at,reverses_id) VALUES('${P}EV05','${P}C004','correction','1.0.0','2025-11-15','$T','${P}EV03');
COMMIT;"

# --- commit validator backstop --------------------------------------------------
rejects "INV-001 unbalanced event is rejected and nothing persists (AT-013)" "INV-001" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV06','${P}C004','card_purchase','1.0.0','2025-10-31','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS09','${P}EV06','${P}AX11','${P}ENT1','USD','debit',1000);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS10','${P}EV06','${P}AC01','${P}ENT1','USD','credit',999);
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C004',3,4,'interpret','smoke','$T');
COMMIT;"
equals "no partial state after the rejected commit" "0" "SELECT count(*) FROM accounting_events WHERE id='${P}EV06';"
rejects "a transaction that never writes its commits row fails at COMMIT" "FOREIGN KEY constraint failed" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV07','${P}C009','card_purchase','1.0.0','2025-10-31','$T');
COMMIT;"
rejects "stale revision is rejected (AT-032)" "AT-032" "INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C005',2,3,'interpret','smoke','$T');"
rejects "seq must equal expected_revision + 1" "CHECK constraint failed: seq = expected_revision + 1" "INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C005',3,5,'interpret','smoke','$T');"
rejects "INV-003 posting to a non-leaf category is rejected" "leaf category" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV08','${P}C004','card_purchase','1.0.0','2025-10-31','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS11','${P}EV08','${P}AX10','${P}ENT1','USD','debit',500);
COMMIT;"
rejects "INV-006 a draft group cannot hold postings" "INV-006" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV09','${P}C004','card_purchase','1.0.0','2025-10-31','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS12','${P}EV09','${P}AX11','${P}ENT1','USD','debit',500);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS13','${P}EV09','${P}AC01','${P}ENT1','USD','credit',500);
INSERT INTO settlement_groups(id,recipe_id,recipe_version,commit_id,recorded_at) VALUES('${P}GRP3','card_purchase','1.0.0','${P}C004','$T');
INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS31','${P}GRP3',1,'draft','${P}C004','$T');
INSERT INTO settlement_postings VALUES('${P}GRP3','${P}PS13','${P}C004');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C004',3,4,'interpret','smoke','$T');
COMMIT;"
rejects "ADR 0006 a posting joins at most one group" "UNIQUE constraint failed: settlement_postings.posting_id" "
BEGIN;
INSERT INTO settlement_groups(id,recipe_id,recipe_version,commit_id,recorded_at) VALUES('${P}GRP4','card_purchase','1.0.0','${P}C004','$T');
INSERT INTO settlement_postings VALUES('${P}GRP4','${P}PS06','${P}C004');
COMMIT;"
rejects "ADR 0004 a second entity row is rejected" "UNIQUE constraint failed: entity.singleton" "INSERT INTO entity(id,party_id,name,created_at) VALUES('${P}ENT2','${P}PTY2','Other','$T');"
rejects "ADR 0003 entities have no visibility column" "no such column: visibility" "SELECT visibility FROM entity;"
rejects "money is integer minor units (STRICT rejects a float)" "cannot store REAL value in INTEGER column" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV10','${P}C004','card_purchase','1.0.0','2025-10-31','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS14','${P}EV10','${P}AX11','${P}ENT1','USD','debit',12.5);
COMMIT;"
rejects "ids must be ULIDs" "CHECK constraint failed" "INSERT INTO parties(id,name,created_at) VALUES('je_1','Bad id','$T');"
rejects "observation natural key is unique (feed + external id)" "UNIQUE constraint failed: observations.feed_id, observations.external_id" "
BEGIN;
INSERT INTO observations(id,feed_id,external_id,financial_account_id,occurred_date,direction,amount_minor,currency,row_hash,payload_json,commit_id,recorded_at)
  VALUES('${P}B002','${P}FED1','synthetic-1','${P}FAC1','2025-10-30','outflow',4250,'USD','$(printf 'a%.0s' {1..64})','{}','${P}C004','$T');
COMMIT;"

rejects "ADR 0002 a void must reverse the linked current event in the same commit" "a void must reverse every current event" "
BEGIN;
INSERT INTO observation_voids(id,observation_id,reason,commit_id,recorded_at) VALUES('${P}V001','${P}B001','duplicate','${P}C004','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C004',3,4,'void','smoke','$T');
COMMIT;"
rejects "ADR 0005 no valuation snapshot on a card account" "valuation snapshots only on non-cash asset accounts" "
BEGIN;
INSERT INTO valuation_snapshots(id,financial_account_id,as_of,value_minor,currency,source,commit_id,recorded_at) VALUES('${P}VS01','${P}FAC1','2025-10-31',100,'USD','manual','${P}C004','$T');
COMMIT;"
rejects "INV-008/AT-030 a reconciled group with a clearing residual is rejected" "nonzero clearing residual" "
BEGIN;
INSERT INTO ledger_accounts(id,entity_id,account_type,kind,name,created_at) VALUES('${P}AC02','${P}ENT1','asset','clearing','Clearing','$T');
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV11','${P}C004','card_purchase','1.0.0','2025-10-31','$T');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS15','${P}EV11','${P}AC02','${P}ENT1','USD','debit',1000);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS16','${P}EV11','${P}AC01','${P}ENT1','USD','credit',1000);
INSERT INTO settlement_groups(id,recipe_id,recipe_version,commit_id,recorded_at) VALUES('${P}GRP5','card_purchase','1.0.0','${P}C004','$T');
INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS51','${P}GRP5',1,'reconciled','${P}C004','$T');
INSERT INTO settlement_postings VALUES('${P}GRP5','${P}PS15','${P}C004');
INSERT INTO settlement_postings VALUES('${P}GRP5','${P}PS16','${P}C004');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C004',3,4,'settle','smoke','$T');
COMMIT;"

# --- PR 19 fix 2: caller-supplied expected_revision (two writers saw revision 3)
ok "writer A (expected_revision 3, from the caller) commits revision 4" "
BEGIN IMMEDIATE;
INSERT INTO user_grants(id,user_id,role,commit_id,recorded_at) VALUES('${P}6G01','${P}H0M1','owner','${P}CA04','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}CA04',3,4,'grant','writer-a','$T');
COMMIT;"
rejects "writer B (also saw revision 3) is rejected as stale (AT-032)" "AT-032: stale revision" "
BEGIN IMMEDIATE;
INSERT INTO user_grants(id,user_id,role,commit_id,recorded_at) VALUES('${P}6G02','${P}H0M2','viewer','${P}CB04','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}CB04',3,4,'grant','writer-b','$T');
COMMIT;"
equals "writer B left nothing behind" "0" "SELECT count(*) FROM user_grants WHERE id='${P}6G02';"

# --- PR 19 Q3: dissolving a reconciled group must reverse (and repost) its legs
rejects "dissolving a reconciled group without reversing its legs is rejected" "must reverse all of its legs" "
BEGIN IMMEDIATE;
INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS22','${P}GRP2',2,'dissolved','${P}C005','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C005',4,5,'correct','smoke','$T');
COMMIT;"
rejects "dissolving with a reversal but no repost of the observed leg is rejected" "must be reposted in the same commit" "
BEGIN IMMEDIATE;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at,reverses_id) VALUES('${P}EV12','${P}C005','correction','1.0.0','2025-10-30','$T','${P}EV03');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS17','${P}EV12','${P}AX12','${P}ENT1','USD','credit',4250);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS18','${P}EV12','${P}AC01','${P}ENT1','USD','debit',4250);
UPDATE accounting_events SET superseded_by='${P}EV12' WHERE id='${P}EV03';
INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS22','${P}GRP2',2,'dissolved','${P}C005','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}C005',4,5,'correct','smoke','$T');
COMMIT;"

# --- PR 19 Q5: reference data is immutable; renames are appended
rejects "in-place rename of a party is rejected" "parties are immutable" "UPDATE parties SET name='Renamed' WHERE id='${P}SHP1';"
rejects "in-place rename of a ledger account is rejected" "ledger accounts are immutable" "UPDATE ledger_accounts SET name='Renamed' WHERE id='${P}AX11';"
ok "rename by appending to reference_renames" "INSERT INTO reference_renames(id,target_kind,target_id,name,recorded_at) VALUES('${P}RN01','party','${P}SHP1','Example Grocer Co','$T');"
equals "current name comes from the latest rename" "Example Grocer Co" "SELECT name FROM reference_current_names WHERE target_id='${P}SHP1';"
rejects "rename of a missing target is rejected" "rename target does not exist" "INSERT INTO reference_renames(id,target_kind,target_id,name,recorded_at) VALUES('${P}RN02','tag','${P}SHP1','x','$T');"

# --- PR 19 nit 3: every commit-scoped table rejects rows for a sealed commit (C001)
S=${P}C001
HB=$(printf 'b%.0s' {1..64}); HC=$(printf 'c%.0s' {1..64})
sealed() { rejects "sealed commit rejects a new $1 row" "commit already sealed" "$2"; }
sealed user_grants "INSERT INTO user_grants(id,user_id,role,commit_id,recorded_at) VALUES('${P}6G09','${P}H0M9','viewer','$S','$T');"
sealed accounting_events "INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV99','$S','card_purchase','1.0.0','2025-10-31','$T');"
sealed evidence_deletions "INSERT INTO evidence_deletions(id,sha256,reason,commit_id,recorded_at) VALUES('${P}ED01','$HB','test','$S','$T');"
sealed evidence_links "INSERT INTO evidence_links(id,document_id,event_id,commit_id) VALUES('${P}E701','${P}D0C1','${P}EV01','$S');"
sealed import_runs "INSERT INTO import_runs(id,feed_id,importer_id,importer_version,commit_id,recorded_at) VALUES('${P}1R01','${P}FED1','monarch','0.1','$S','$T');"
sealed observations "INSERT INTO observations(id,feed_id,external_id,financial_account_id,occurred_date,direction,amount_minor,currency,row_hash,payload_json,commit_id,recorded_at) VALUES('${P}B009','${P}FED1','synthetic-9','${P}FAC1','2025-10-31','outflow',100,'USD','$HC','{}','$S','$T');"
sealed observation_voids "INSERT INTO observation_voids(id,observation_id,reason,commit_id,recorded_at) VALUES('${P}V009','${P}B001','duplicate','$S','$T');"
sealed settlement_groups "INSERT INTO settlement_groups(id,recipe_id,recipe_version,commit_id,recorded_at) VALUES('${P}GRP9','card_purchase','1.0.0','$S','$T');"
sealed settlement_group_states "INSERT INTO settlement_group_states(id,group_id,seq,state,commit_id,recorded_at) VALUES('${P}GS29','${P}GRP2',2,'dissolved','$S','$T');"
sealed settlement_postings "INSERT INTO settlement_postings VALUES('${P}GRP2','${P}PS05','$S');"
sealed obligations "INSERT INTO obligations(id,party_id,direction,ledger_account_id,currency,original_minor,opened_event_id,commit_id,recorded_at) VALUES('${P}0B09','${P}SHP1','receivable','${P}AR01','USD',100,'${P}EV0A','$S','$T');"
sealed obligation_settlements "INSERT INTO obligation_settlements(id,obligation_id,event_id,kind,amount_minor,commit_id,recorded_at) VALUES('${P}0S01','${P}0B01','${P}EV0A','payment',100,'$S','$T');"
sealed cross_entity_outbox "INSERT INTO cross_entity_outbox(id,recipe_instance_id,idempotency_key,counterpart_party_id,action,amount_minor,currency,event_id,commit_id,recorded_at) VALUES('${P}0X01','${P}R101','k1','${P}SHP1','leg',100,'USD','${P}EV01','$S','$T');"
sealed cross_entity_inbox "INSERT INTO cross_entity_inbox(idempotency_key,recipe_instance_id,source_party_id,action,amount_minor,currency,event_id,commit_id,recorded_at) VALUES('k2','${P}R102','${P}SHP1','leg',100,'USD','${P}EV01','$S','$T');"
sealed statements "INSERT INTO statements(id,financial_account_id,period_start,period_end,commit_id,recorded_at) VALUES('${P}ST01','${P}FAC1','2025-10-01','2025-10-31','$S','$T');"
sealed balance_assertions "INSERT INTO balance_assertions(id,financial_account_id,cutoff,balance_minor,currency,source,commit_id,recorded_at) VALUES('${P}BA01','${P}FAC1','2025-10-31',4250,'USD','manual','$S','$T');"
sealed valuation_snapshots "INSERT INTO valuation_snapshots(id,financial_account_id,as_of,value_minor,currency,source,commit_id,recorded_at) VALUES('${P}VS09','${P}FAB1','2025-10-31',100,'USD','manual','$S','$T');"
sealed journal_postings "INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS99','${P}EV01','${P}AX11','${P}ENT1','USD','debit',1);"
sealed posting_tags "INSERT INTO posting_tags VALUES('${P}PS01','${P}TG01');"
sealed posting_observations "INSERT INTO posting_observations VALUES('${P}PS02','${P}B001');"
sealed refund_links "INSERT INTO refund_links(id,refund_posting_id,purchase_posting_id,amount_minor) VALUES('${P}R701','${P}PS03','${P}PS01',100);"
sealed obligation_postings "INSERT INTO obligation_postings VALUES('${P}0B01','${P}PS01');"
sealed settlement_draft_participants "INSERT INTO settlement_draft_participants(id,group_state_id,role,expected_json) VALUES('${P}DP01','${P}GS11','expected','{}');"
sealed kernel_state "INSERT INTO kernel_state(id,state_key,active,code,commit_id,recorded_at) VALUES('${P}FRZ9','freeze',1,'UnsealedRows','$S','$T');"
equals "all 24 commit-scoped tables are covered by doctor_unsealed_rows" "24|24" "SELECT count(*), sum(instr((SELECT sql FROM sqlite_master WHERE name='doctor_unsealed_rows'), char(39)||t||char(39)) > 0) FROM (SELECT DISTINCT tbl_name AS t FROM sqlite_master WHERE type='trigger' AND name GLOB '*_open_commit');"

# --- read models ----------------------------------------------------------------
equals "register hides the superseded entry and its reversal" "${P}EV03
${P}EV0A" "SELECT id FROM register_events ORDER BY id;"
equals "audit keeps all events with lineage" "${P}EV01|${P}EV02
${P}EV02|
${P}EV03|
${P}EV0A|" "SELECT id, coalesce(superseded_by,'') FROM accounting_events ORDER BY id;"
equals "balances are the plain sum of all postings" "${P}AC01|5250
${P}AR01|1000
${P}AX11|0
${P}AX12|4250" "SELECT ledger_account_id, balance_minor FROM account_balances ORDER BY 1;"
equals "trial balance closes per currency (AT-034)" "0" "SELECT sum(net_debit_minor) FROM trial_balance WHERE currency='USD';"
equals "doctor views are clean" "0|0|0|0" "SELECT (SELECT count(*) FROM doctor_unbalanced_events),(SELECT count(*) FROM doctor_superseded_drift),(SELECT count(*) FROM doctor_observation_double_consumed),(SELECT count(*) FROM doctor_unsealed_rows);"
equals "integrity and foreign keys clean" "ok" "PRAGMA integrity_check; PRAGMA foreign_key_check;"

# --- U17: kernel_state freeze row, commits.kind includes freeze -----------------
ok "U17 freeze commit seals a kernel_state row" "
BEGIN;
INSERT INTO kernel_state(id,state_key,active,code,commit_id,recorded_at) VALUES('${P}FRZ1','freeze',1,'UnsealedRows','${P}CFR1','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}CFR1',(SELECT coalesce(max(seq),0) FROM commits),(SELECT coalesce(max(seq),0) FROM commits)+1,'freeze','smoke','$T');
COMMIT;"
rejects "U17 kernel_state is append-only" "never updated" "UPDATE kernel_state SET active=0 WHERE id='${P}FRZ1';"
rejects "U17 kernel_state is never deleted" "never deleted" "DELETE FROM kernel_state WHERE id='${P}FRZ1';"
ok "U17 clear appends active=0 and leaves the prior row" "
BEGIN;
INSERT INTO kernel_state(id,state_key,active,code,commit_id,recorded_at) VALUES('${P}FRZ2','freeze',0,'UnsealedRows','${P}CFR2','$T');
INSERT INTO commits(id,expected_revision,seq,kind,actor,recorded_at) VALUES('${P}CFR2',(SELECT coalesce(max(seq),0) FROM commits),(SELECT coalesce(max(seq),0) FROM commits)+1,'freeze','smoke','$T');
COMMIT;"
equals "U17 latest freeze row is the clear" "0" "SELECT k.active FROM kernel_state k JOIN commits c ON c.id = k.commit_id WHERE k.state_key='freeze' ORDER BY c.seq DESC LIMIT 1;"
equals "U17 clear does not update the prior freeze row" "1" "SELECT active FROM kernel_state WHERE id='${P}FRZ1';"


# --- PR 19 fix 1: a connection with foreign_keys OFF can leave an unsealed row; doctor finds it
if sqlite3 -bail "$DB" "PRAGMA foreign_keys=OFF;" "INSERT INTO user_grants(id,user_id,role,commit_id,recorded_at) VALUES('${P}6G0F','${P}H0MF','viewer','${P}CZZZ','$T');" 2>&1; then
  echo "PASS  with foreign_keys OFF an orphan commit-scoped row is accepted (why open() must assert the pragma)"; pass=$((pass+1))
else echo "FAIL  orphan insert with foreign_keys OFF unexpectedly failed"; fail=$((fail+1)); fi
equals "doctor_unsealed_rows reports the orphan" "user_grants|${P}CZZZ" "SELECT table_name, commit_id FROM doctor_unsealed_rows;"
equals "doctor PRAGMA foreign_key_check reports it too" "user_grants" "SELECT DISTINCT \"table\" FROM pragma_foreign_key_check;"

echo "---"; echo "passed=$pass failed=$fail"
[[ $fail -eq 0 ]]
