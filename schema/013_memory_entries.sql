-- M14_001: Zombie agent memory — schema, role, table DDL, and grants.
-- Survives workspace destruction. Isolated from core.* via the memory_runtime role.
-- See docs/v2/active/M14_001_PERSISTENT_ZOMBIE_MEMORY.md.
-- Confused-deputy mitigation per RULE CTX: memory lives behind a process boundary
-- (Postgres role with no grants on core.*), NOT a shared filesystem.
--
-- Row-level isolation: memory_runtime connects with instance_id="zmb:{uuid}"
-- (set by zombie_memory.zig). All queries scope by instance_id.

-- memory_runtime role is created in schema/004_vault_schema.sql.
-- This migration scopes it to the memory schema and grants it CREATE
-- so NullClaw can run its own idempotent DDL on first executor connect.
GRANT USAGE, CREATE ON SCHEMA memory TO memory_runtime;

-- Allow api_runtime to SET ROLE memory_runtime in HTTP handlers.
-- RULE CTX: api_runtime still has zero direct grants on memory.*
-- (those belong to memory_runtime). SET ROLE gives temporary memory
-- access within a single request; RESET ROLE restores api_runtime.
GRANT memory_runtime TO api_runtime;

-- memory_runtime search_path is scoped to its own schema.
ALTER ROLE memory_runtime SET search_path = memory, public;

-- memory_entries: primary store for zombie persistent memory.
-- Column layout matches NullClaw's PostgresMemory schema exactly so NullClaw's
-- own CREATE TABLE IF NOT EXISTS / CREATE UNIQUE INDEX IF NOT EXISTS are no-ops.
-- id: "{nanoseconds}-{hex64}-{hex64}" (same format as NullClaw's generateId)
-- created_at / updated_at: decimal Unix epoch as TEXT (same as NullClaw's getNowTimestamp)
CREATE TABLE IF NOT EXISTS memory.memory_entries (
    id          TEXT PRIMARY KEY,
    key         TEXT NOT NULL,
    content     TEXT NOT NULL,
    category    TEXT NOT NULL DEFAULT 'core',
    session_id  TEXT,
    instance_id TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Required for ON CONFLICT (key, instance_id) upserts in memory_http.zig.
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_entries_key_instance
    ON memory.memory_entries(key, instance_id);

CREATE INDEX IF NOT EXISTS idx_memory_entries_category
    ON memory.memory_entries(category);

CREATE INDEX IF NOT EXISTS idx_memory_entries_instance
    ON memory.memory_entries(instance_id);

-- Explicit grants now that we own the table DDL.
GRANT SELECT, INSERT, UPDATE, DELETE ON memory.memory_entries TO memory_runtime;

-- Negative-test guarantee: nothing else can read memory tables.
REVOKE CREATE ON SCHEMA memory FROM PUBLIC;
