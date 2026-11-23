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
INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C001',1,'setup','smoke','$T');
COMMIT;"

ok "commit 2: import + card purchase (debit Groceries, credit Card) reconciled in one commit" "
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
INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C002',2,'interpret','smoke','$T');
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
INSERT INTO commits(id,seq,kind,actor,reason,recorded_at) VALUES('${P}C003',3,'correct','smoke','recategorize','$T');
COMMIT;"

rejects "superseded_by cannot be set a second time" "only superseded_by may be set" "UPDATE accounting_events SET superseded_by='${P}EV03' WHERE id='${P}EV01';"
rejects "superseded_by cannot be cleared" "only superseded_by may be set" "UPDATE accounting_events SET superseded_by=NULL WHERE id='${P}EV01';"
rejects "superseded_by cannot be set after the reversal's commit sealed" "only superseded_by may be set" "UPDATE accounting_events SET superseded_by='${P}EV02' WHERE id='${P}EV01';"
rejects "a reversal must flag its target in the same commit" "must be flagged superseded_by" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at,reverses_id) VALUES('${P}EV04','${P}C004','correction','1.0.0','2025-10-30','$T','${P}EV03');
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS07','${P}EV04','${P}AX12','${P}ENT1','USD','credit',4250);
INSERT INTO journal_postings(id,event_id,ledger_account_id,entity_id,currency,side,amount_minor) VALUES('${P}PS08','${P}EV04','${P}AC01','${P}ENT1','USD','debit',4250);
INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C004',4,'correct','smoke','$T');
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
INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C004',4,'interpret','smoke','$T');
COMMIT;"
equals "no partial state after the rejected commit" "0" "SELECT count(*) FROM accounting_events WHERE id='${P}EV06';"
rejects "a transaction that never writes its commits row fails at COMMIT" "FOREIGN KEY constraint failed" "
BEGIN;
INSERT INTO accounting_events(id,commit_id,recipe_id,recipe_version,effective_date,recorded_at) VALUES('${P}EV07','${P}C009','card_purchase','1.0.0','2025-10-31','$T');
COMMIT;"
rejects "stale revision is rejected (AT-032)" "AT-032" "INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C005',3,'interpret','smoke','$T');"
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
INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C004',4,'interpret','smoke','$T');
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
INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C004',4,'void','smoke','$T');
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
INSERT INTO commits(id,seq,kind,actor,recorded_at) VALUES('${P}C004',4,'settle','smoke','$T');
COMMIT;"

# --- read models ----------------------------------------------------------------
equals "register hides the superseded entry and its reversal" "${P}EV03" "SELECT id FROM register_events;"
equals "audit keeps all three events with lineage" "${P}EV01|${P}EV02
${P}EV02|
${P}EV03|" "SELECT id, coalesce(superseded_by,'') FROM accounting_events ORDER BY id;"
equals "balances are the plain sum of all postings" "${P}AC01|4250
${P}AX11|0
${P}AX12|4250" "SELECT ledger_account_id, balance_minor FROM account_balances ORDER BY 1;"
equals "trial balance closes per currency (AT-034)" "0" "SELECT sum(net_debit_minor) FROM trial_balance WHERE currency='USD';"
equals "doctor views are clean" "0|0|0" "SELECT (SELECT count(*) FROM doctor_unbalanced_events),(SELECT count(*) FROM doctor_superseded_drift),(SELECT count(*) FROM doctor_observation_double_consumed);"
equals "integrity and foreign keys clean" "ok" "PRAGMA integrity_check; PRAGMA foreign_key_check;"

echo "---"; echo "passed=$pass failed=$fail"
[[ $fail -eq 0 ]]
