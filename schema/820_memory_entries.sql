-- Fleet memory: isolated from `core` by role, and erased with the fleet it
-- belongs to.
--
-- Confused-deputy mitigation per RULE CTX: memory lives behind a process
-- boundary — a PostgreSQL role with no grants on `core` — not a shared
-- filesystem. `fleet_id` REFERENCES `core.fleets` ON DELETE CASCADE, and that
-- edge does NOT weaken the boundary: REFERENCES is a schema-definition
-- privilege held by the migrator, and PostgreSQL evaluates both the check and
-- the cascade with the table owner's authority. `memory_runtime` gains no
-- `core` grant and still cannot name `core.fleets` at all.
--
-- The edge exists because the alternative left rows nobody could reach. Erasure
-- and fleet deletion still name this table explicitly, but an explicit sweep is
-- scoped by a fleet the caller enumerated — so a row whose fleet was already
-- gone was unreachable by every one of them, and an erased account kept its
-- memory permanently. A capture racing an erasure now either commits before the
-- fleet row goes and cascades away, or blocks on that row's lock and fails
-- closed on the missing parent. The explicit sweeps stay as belt and braces.
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
    -- Named rather than left to PostgreSQL's `memory_entries_fleet_id_fkey`, so a
    -- database that gained this edge by hand carries the same constraint name as
    -- one rebuilt from these slots.
    fleet_id    UUID   NOT NULL
        CONSTRAINT fk_memory_entries_fleet_id REFERENCES core.fleets(id) ON DELETE CASCADE,
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
