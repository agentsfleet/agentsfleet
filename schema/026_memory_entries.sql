-- M14_001: Zombie agent memory — durable store for `core` and `daily` categories.
-- Survives workspace destruction. Isolated from core.* via the memory_runtime role.
-- See docs/v2/active/M14_001_PERSISTENT_ZOMBIE_MEMORY.md.
-- Confused-deputy mitigation per RULE CTX: memory lives behind a process boundary
-- (Postgres role with no grants on core.*), NOT a shared filesystem.

CREATE TABLE IF NOT EXISTS memory.memory_entries (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_memory_entries_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id       UUID NOT NULL REFERENCES core.zombies(id),
    category        TEXT NOT NULL,
    key             TEXT NOT NULL,
    content         TEXT NOT NULL,
    tags            TEXT[] NOT NULL DEFAULT '{}',
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT ck_memory_entries_category
        CHECK (category IN ('core', 'daily', 'conversation', 'workspace')),
    CONSTRAINT ck_memory_entries_key_len
        CHECK (char_length(key) BETWEEN 1 AND 255),
    CONSTRAINT ck_memory_entries_content_len
        CHECK (char_length(content) BETWEEN 1 AND 16384),
    CONSTRAINT ck_memory_entries_tags_count
        CHECK (coalesce(array_length(tags, 1), 0) <= 32),
    CONSTRAINT uq_memory_entries_scope
        UNIQUE (zombie_id, category, key)
);

-- Primary lookup: recall/list by zombie + category.
CREATE INDEX IF NOT EXISTS idx_memory_entries_zombie_category
    ON memory.memory_entries(zombie_id, category);

-- Recency ordering for "last N entries" and retention pruning.
CREATE INDEX IF NOT EXISTS idx_memory_entries_zombie_updated
    ON memory.memory_entries(zombie_id, updated_at DESC);

-- Tag filter for memory_list(tag=...) queries.
CREATE INDEX IF NOT EXISTS idx_memory_entries_tags
    ON memory.memory_entries USING GIN (tags);

-- Runtime grants: memory_runtime is the ONLY runtime role with access.
-- worker_runtime and api_runtime intentionally have zero grants here.
-- Any agent shell tool running under a different role cannot reach memory.
GRANT USAGE ON SCHEMA memory TO memory_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON memory.memory_entries TO memory_runtime;

-- memory_runtime search_path is scoped to its own schema.
ALTER ROLE memory_runtime SET search_path = memory, public;

-- Negative-test guarantee: nothing else can read memory tables.
REVOKE ALL ON ALL TABLES IN SCHEMA memory FROM PUBLIC;
REVOKE CREATE ON SCHEMA memory FROM PUBLIC;
