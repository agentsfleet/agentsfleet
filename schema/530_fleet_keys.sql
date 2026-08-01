-- Workspace-scoped fleet keys for external callers (LangGraph, CrewAI, Composio).
-- Each external fleet gets a companion fleet record, so the integration-grant
-- system applies identically to internal and external callers.
--
-- `key_hash` is the SHA-256 hex of the raw `agt_a` key. The raw value is shown
-- once at creation and never retrievable again, so this table holds no
-- credential material (RULE VLT).
--
-- The retired shape carried a generated UUID primary key plus `fleet_key_id TEXT NOT
-- NULL UNIQUE`, a CHECK tying the two to the same value, and a full-shape UUID
-- regular expression on the text twin. All three go: the value is one column
-- now, and the version nibble check below is the smoke alarm the identity slot
-- argues for — a full-shape regular expression was measured at 17x its per-row
-- cost to re-catch what the generator's own tests already catch. The public
-- field name `fleet_key_id` is unchanged; it is aliased at the boundary, per the
-- identity rule in SCHEMA_CONVENTIONS.

CREATE TABLE IF NOT EXISTS core.fleet_keys (
    id            UUID   PRIMARY KEY,
    CONSTRAINT ck_fleet_keys_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id  UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    fleet_id      UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    name          TEXT   NOT NULL,
    description   TEXT   NOT NULL,
    key_hash      TEXT   NOT NULL,
    last_used_at  BIGINT,
    created_at    BIGINT NOT NULL,
    -- One key per companion fleet: the fleet record exists to carry this key, so
    -- a second would make grant resolution ambiguous.
    CONSTRAINT uq_fleet_keys_fleet_id UNIQUE (fleet_id),
    -- The authentication lookup filters on the hash alone, so this unique
    -- constraint is that whole access path as well as its integrity guarantee.
    CONSTRAINT uq_fleet_keys_key_hash UNIQUE (key_hash)
);

-- No `updated_at`: a key is created, used, and deleted. `name` and `description`
-- are set once at creation and nothing edits them, so the only column that
-- changes after INSERT is `last_used_at`, which carries its own domain time.

-- Reader: the workspace's key list, and the index the erasure cascade walks.
-- Neither unique constraint above leads with `workspace_id`, so this is not a
-- duplicate of either.
CREATE INDEX IF NOT EXISTS idx_fleet_keys_workspace_id
    ON core.fleet_keys (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_keys TO api_runtime;
