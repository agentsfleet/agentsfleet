-- Fleet memory: the store that survives workspace destruction, isolated from
-- `core` by role rather than by convention.
--
-- Confused-deputy mitigation per RULE CTX: memory lives behind a process
-- boundary — a PostgreSQL role with no grants on `core` — not a shared
-- filesystem. The table deliberately carries NO foreign key to `core.fleets`:
-- the role isolation is the boundary, and a cross-schema foreign key would
-- couple memory back to core, which is the thing being prevented.
--
-- That has a consequence this rebuild states rather than leaves implicit. Every
-- other child table now resolves to a tenant through a cascade, so account
-- erasure no longer names it. Memory has no such edge, so erasure and fleet
-- deletion still delete from this table EXPLICITLY. It is the one table that
-- genuinely belongs in the hand-maintained delete order, and it
-- is there because of the trust boundary, not by omission.
--
-- Scope: every row belongs to one fleet. The runner-memory adapter derives
-- `fleet_id` from the lease it issued and scopes every query by it — never a
-- fetch-all followed by an in-memory filter.
--
-- The retired shape carried a generated UUID primary key alongside `id TEXT NOT NULL
-- UNIQUE`, the latter in an inherited "{nanoseconds}-{hex}-{hex}" format. Unlike
-- the twins elsewhere in this rebuild those held different values, so the text
-- column was not a duplicate — it was simply never read by anything outside the
-- database. The memory surface selects `key, content, category`; the text id
-- appeared only as an ORDER BY tiebreak and in its own eviction subquery. It is
-- gone, and the UUID does that job: a hundred and forty bytes per row become
-- sixteen, on a table bounded only by the per-fleet cap, and a version 7
-- identifier sorts in creation order where the inherited format did not.

CREATE TABLE IF NOT EXISTS memory.memory_entries (
    id          UUID   PRIMARY KEY,
    CONSTRAINT ck_memory_entries_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    key         TEXT   NOT NULL,
    content     TEXT   NOT NULL,
    category    TEXT   NOT NULL,
    fleet_id    UUID   NOT NULL,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL,
    -- The fleet's own overwrite mechanism and the upsert's conflict target: a
    -- repeated key replaces rather than accumulates, which is the primary bound
    -- on a fleet's memory growth.
    CONSTRAINT uq_memory_entries_key_fleet_id UNIQUE (key, fleet_id)
);

-- Reader: hydration (a fleet's whole memory set, newest first) and the
-- past-the-cap eviction, both ordering by updated_at with the identifier as
-- tiebreak.
CREATE INDEX IF NOT EXISTS idx_memory_entries_fleet_id_updated_at_id
    ON memory.memory_entries (fleet_id, updated_at DESC, id DESC);

-- Reader: the tenant memory list, newest-created first with the per-fleet-unique
-- key as tiebreak, so entries sharing a created_at millisecond are never skipped
-- across a page boundary.
CREATE INDEX IF NOT EXISTS idx_memory_entries_fleet_id_created_at_key
    ON memory.memory_entries (fleet_id, created_at DESC, key DESC);

-- Reader: the retention sweep — DELETE WHERE fleet_id = $1 AND category = $2
-- AND updated_at < $3. The retired index was a bare btree on `category`, a
-- column holding a handful of values across every fleet in the deployment, so it
-- offered the planner almost no selectivity and the sweep fell back to filtering
-- by age after the fact. This composite matches the statement's own shape:
-- equality on fleet, equality on category, then the age range last, which is the
-- order one index scan can use for all three.
CREATE INDEX IF NOT EXISTS idx_memory_entries_fleet_id_category_updated_at
    ON memory.memory_entries (fleet_id, category, updated_at);

-- No bare `fleet_id` index: all three composites above lead with that column, so
-- one would answer nothing they cannot.

-- memory_runtime owns the table; api_runtime holds nothing here and must assume
-- the role (membership is granted in schema/110). The secret store and the
-- wallet follow this same pattern.
GRANT SELECT, INSERT, UPDATE, DELETE ON memory.memory_entries TO memory_runtime;
