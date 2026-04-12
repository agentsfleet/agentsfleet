-- M14_001: Zombie agent memory — schema, role, and grants.
-- Survives workspace destruction. Isolated from core.* via the memory_runtime role.
-- See docs/v2/active/M14_001_PERSISTENT_ZOMBIE_MEMORY.md.
-- Confused-deputy mitigation per RULE CTX: memory lives behind a process boundary
-- (Postgres role with no grants on core.*), NOT a shared filesystem.
--
-- NOTE: NullClaw's PostgresMemory.init() auto-migrates the actual tables
-- (memory_entries, messages, session_usage) on first connect. This migration
-- only creates the schema and role; table DDL is intentionally delegated to
-- NullClaw so its column layout (instance_id TEXT, TIMESTAMPTZ timestamps, etc.)
-- stays coherent with NullClaw's internal queries. Steps 5+ (export/import)
-- can ALTER TABLE to add operator-visible columns (e.g. tags[]) without
-- conflicting with NullClaw's required columns.
--
-- Row-level isolation: memory_runtime connects with instance_id="zmb:{uuid}"
-- (set by zombie_memory.zig). NullClaw's queries all scope by instance_id,
-- so two concurrent zombies cannot read each other's entries.

-- memory_runtime role is created in schema/004_vault_schema.sql.
-- This migration scopes it to the memory schema and grants it CREATE
-- so NullClaw can auto-migrate its tables on first connect.
GRANT USAGE, CREATE ON SCHEMA memory TO memory_runtime;

-- Auto-grant on any tables NullClaw creates. This covers memory_entries,
-- messages, and session_usage which NullClaw auto-creates at runtime.
ALTER DEFAULT PRIVILEGES IN SCHEMA memory
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO memory_runtime;

ALTER DEFAULT PRIVILEGES IN SCHEMA memory
    GRANT USAGE, SELECT ON SEQUENCES TO memory_runtime;

-- memory_runtime search_path is scoped to its own schema.
ALTER ROLE memory_runtime SET search_path = memory, public;

-- Negative-test guarantee: nothing else can read memory tables.
REVOKE CREATE ON SCHEMA memory FROM PUBLIC;
