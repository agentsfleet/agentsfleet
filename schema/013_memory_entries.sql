-- Zombie agent memory — schema, role, table DDL, and grants.
-- Survives workspace destruction. Isolated from core.* via the memory_runtime role.
-- Confused-deputy mitigation per RULE CTX: memory lives behind a process boundary
-- (Postgres role with no grants on core.*), NOT a shared filesystem. The table
-- deliberately carries NO foreign key to core.zombies — the role isolation is the
-- boundary, and a cross-schema FK would couple memory to core.
--
-- Scope: every row belongs to one zombie (zombie_id). zombied's runner-memory
-- adapter derives zombie_id from the lease it issued and scopes every query
-- WHERE zombie_id = $1 (never a fetch-all + in-memory filter). zombied fully owns
-- this table — the in-child NullClaw Postgres memory path is retired, so the
-- column layout no longer has to mirror NullClaw's PostgresMemory schema.
--
-- Pre-v2.0 teardown convention: migrations are edited in place and the test/dev
-- DB is rebuilt from scratch (no live-data preservation, no ALTER chain) — so the
-- instance_id -> zombie_id column change here is a fresh CREATE, not an in-place
-- ALTER. The schema-gate enforces this teardown posture while VERSION < 2.0.0.

-- memory_runtime role is created in the vault schema (002_vault_schema.sql).
-- This migration scopes it to the memory schema. USAGE only: zombied owns the
-- table DDL, so the role no longer needs CREATE (the in-child NullClaw self-DDL
-- on first runner connect is gone).
GRANT USAGE ON SCHEMA memory TO memory_runtime;

-- Allow api_runtime to SET ROLE memory_runtime in HTTP handlers.
-- RULE CTX: api_runtime still has zero direct grants on memory.*
-- (those belong to memory_runtime). SET ROLE gives temporary memory
-- access within a single request; RESET ROLE restores api_runtime.
GRANT memory_runtime TO api_runtime;

-- memory_runtime search_path is scoped to its own schema.
ALTER ROLE memory_runtime SET search_path = memory, public;

-- memory_entries: primary store for zombie persistent memory.
-- id: "{nanoseconds}-{hex64}-{hex64}" (NullClaw generateId format, retained).
-- created_at / updated_at: BIGINT Unix epoch milliseconds (project clock),
-- per the platform timestamp convention — set at INSERT, updated_at refreshed
-- on every upsert. Age arithmetic (the daily retention sweep) rides updated_at.
-- zombie_id: the owning zombie (UUID) — our identifier end to end, no "zmb:" form.
CREATE TABLE IF NOT EXISTS memory.memory_entries (
    uid         UUID PRIMARY KEY,
    CONSTRAINT ck_memory_entries_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id          TEXT NOT NULL UNIQUE,
    key         TEXT NOT NULL,
    content     TEXT NOT NULL,
    category    TEXT NOT NULL,
    zombie_id   UUID NOT NULL,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);

-- Required for ON CONFLICT (key, zombie_id) upserts in the runner-memory adapter.
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_entries_key_zombie
    ON memory.memory_entries(key, zombie_id);

CREATE INDEX IF NOT EXISTS idx_memory_entries_category
    ON memory.memory_entries(category);

CREATE INDEX IF NOT EXISTS idx_memory_entries_zombie
    ON memory.memory_entries(zombie_id);

-- Explicit grants now that we own the table DDL.
GRANT SELECT, INSERT, UPDATE, DELETE ON memory.memory_entries TO memory_runtime;

-- Negative-test guarantee: nothing else can read memory tables.
REVOKE CREATE ON SCHEMA memory FROM PUBLIC;
