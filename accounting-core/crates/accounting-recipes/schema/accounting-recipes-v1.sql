-- =============================================================================
-- ACCOUNTING RECIPES EXTENSION SCHEMA (Application Ring)
-- =============================================================================
-- Stores workflow provenance and input parameters without polluting the
-- double-entry Core Kernel tables.
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS recipe_executions (
    id               TEXT PRIMARY KEY,
    recipe_name      TEXT NOT NULL,
    recipe_version   TEXT NOT NULL,
    journal_entry_id TEXT REFERENCES journal_entries(id) ON DELETE SET NULL,
    parameters_json  TEXT NOT NULL,
    executed_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_recipe_executions_entry ON recipe_executions(journal_entry_id);