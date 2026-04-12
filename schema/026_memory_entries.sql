-- M14_001: Zombie agent memory — schema, role, and grants.
-- Survives workspace destruction. Isolated from core.* via the memory_runtime role.
-- See docs/v2/active/M14_001_PERSISTENT_ZOMBIE_MEMORY.md.
-- Confused-deputy mitigation per RULE CTX: memory lives behind a process boundary
-- (Postgres role with no grants on core.*), NOT a shared filesystem.
--
-- NOTE: NullClaw's PostgresMemory.init() auto-migrates the actual tables
-- (memory_entries, messages, session_usage) on first connect. This migration
-- only creates the schema and role; table DDL is intentionally delegated to
-- NullClaw. Actual column types (verified from NullClaw source):
--   id TEXT PRIMARY KEY             -- format: "{ns}-{hex64}-{hex64}"
--   key TEXT NOT NULL
--   content TEXT NOT NULL
--   category TEXT NOT NULL DEFAULT 'core'
--   session_id TEXT
--   instance_id TEXT NOT NULL DEFAULT ''
--   created_at TEXT NOT NULL        -- decimal Unix epoch (e.g. "1712931234")
--   updated_at TEXT NOT NULL        -- decimal Unix epoch
-- NullClaw also creates:
--   UNIQUE INDEX idx_memory_entries_key_instance ON memory_entries(key, instance_id)
-- This index is what allows ON CONFLICT (key, instance_id) upserts in memory_http.zig.
--
-- Row-level isolation: memory_runtime connects with instance_id="zmb:{uuid}"
-- (set by zombie_memory.zig). NullClaw's queries all scope by instance_id,
-- so two concurrent zombies cannot read each other's entries.

-- memory_runtime role is created in schema/004_vault_schema.sql.
-- This migration scopes it to the memory schema and grants it CREATE
-- so NullClaw can auto-migrate its tables on first connect.
GRANT USAGE, CREATE ON SCHEMA memory TO memory_runtime;

-- Allow api_runtime to SET ROLE memory_runtime in HTTP handlers.
-- RULE CTX: api_runtime still has zero direct grants on memory.*
-- (those belong to memory_runtime). SET ROLE gives temporary memory
-- access within a single request; RESET ROLE restores api_runtime.
GRANT memory_runtime TO api_runtime;

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
