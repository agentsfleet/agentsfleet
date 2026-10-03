-- The two shared-memory grants a workspace admin sets on a fleet.
--
-- Sharing is a grant and never the default. `memory_reads_workspace` lets a
-- fleet hydrate and recall what other fleets in its workspace published;
-- `memory_publishes_workspace` lets it store an entry the workspace reads.
-- Both false on every existing fleet, so no fleet's memory changes reach by
-- this upgrade — a per-channel resident stays private by staying ungranted.
--
-- Booleans rather than a vocabulary column (RULE STS). Read on the api role
-- before the store takes `memory_runtime`, which cannot see `core`.
--
-- Idempotent: a rerun finds both columns and adds nothing.

ALTER TABLE core.fleets
    ADD COLUMN IF NOT EXISTS memory_reads_workspace BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE core.fleets
    ADD COLUMN IF NOT EXISTS memory_publishes_workspace BOOLEAN NOT NULL DEFAULT false;

-- RULE SGR: the access route updates both columns and the memory verbs read
-- them, as `api_runtime`. The table-level grant from schema/500 already covers
-- them; restated so this slot carries the privilege its callers exercise.
GRANT SELECT, UPDATE ON core.fleets TO api_runtime;
