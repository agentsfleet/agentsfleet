-- A memory entry names its workspace, and says whether that workspace reads it.
--
-- Until this slot an entry was keyed by its fleet alone, so a new fleet started
-- empty even when other fleets in the same workspace had learned what it needs.
-- Two additive columns, and the identity does not move: the unique
-- `(key, fleet_id)` already IS `(workspace_id, fleet_id, key)`, because a fleet
-- belongs to exactly one workspace. The writer stays in the identity, so two
-- fleets publishing the same key are two entries and never a conflict.
--
-- 1. `workspace_id` is backfilled from the fleet each row already names, then
--    made NOT NULL and cascaded with the workspace — the same erasure edge
--    `fk_memory_entries_fleet_id` gives the fleet (schema/820). Denormalised on
--    purpose: `memory_runtime` cannot read `core`, so a workspace-wide read has
--    to find its rows without a join it is not allowed to make.
--
-- 2. `workspace_visible` is a boolean, never a defaulted vocabulary string
--    (RULE STS). False, the fleet-only reading every existing row already has,
--    so no existing entry becomes visible to another fleet by this upgrade.
--
-- Idempotent end to end: a deploy retried after any statement reruns cleanly
-- and changes nothing a first run already changed.

ALTER TABLE memory.memory_entries ADD COLUMN IF NOT EXISTS workspace_id UUID;

ALTER TABLE memory.memory_entries
    ADD COLUMN IF NOT EXISTS workspace_visible BOOLEAN NOT NULL DEFAULT false;

-- Run as the migrator, which may read both schemas; `memory_runtime` never
-- sees this join. Only rows still unset, so a rerun touches nothing.
UPDATE memory.memory_entries AS entry
SET workspace_id = fleet.workspace_id
FROM core.fleets AS fleet
WHERE entry.fleet_id = fleet.id
  AND entry.workspace_id IS NULL;

ALTER TABLE memory.memory_entries ALTER COLUMN workspace_id SET NOT NULL;

-- Named, and added only when absent, so a rerun is a no-op rather than a
-- duplicate-constraint failure. The lookup is by name within this table.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_memory_entries_workspace_id'
          AND conrelid = 'memory.memory_entries'::regclass
    ) THEN
        ALTER TABLE memory.memory_entries
            ADD CONSTRAINT fk_memory_entries_workspace_id
            FOREIGN KEY (workspace_id) REFERENCES core.workspaces(id) ON DELETE CASCADE;
    END IF;
END
$$;

-- Reader: a granted fleet's hydrate and recall, which read the workspace's
-- shared entries newest first. Partial, so the index holds only what is
-- shared — most entries never are.
CREATE INDEX IF NOT EXISTS idx_memory_entries_workspace_shared_updated_at
    ON memory.memory_entries (workspace_id, updated_at DESC, id DESC)
    WHERE workspace_visible;

-- RULE SGR: the store reads and writes both new columns as `memory_runtime`.
-- The table-level grant from schema/820 already covers them; restated so this
-- slot carries the privilege its callers exercise.
GRANT SELECT, INSERT, UPDATE, DELETE ON memory.memory_entries TO memory_runtime;
