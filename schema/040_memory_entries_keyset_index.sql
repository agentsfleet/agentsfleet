-- Keyset ordering for the tenant memory list: newest-created first, with the
-- per-fleet-unique key as the tiebreaker so entries sharing one created_at
-- millisecond are never skipped across page boundaries. The reads seek
-- WHERE (created_at, key) < ($after_ts, $after_key) ORDER BY created_at DESC,
-- key DESC, which this index serves directly. The slot-033 index on
-- (fleet_id, updated_at DESC, id DESC) keeps serving updated_at-ordered
-- consumers (the retention sweep) and is untouched.
--
-- Additive-only: one index, no table or column change, no row rewrite.
-- RULE SGR: no GRANT lines — an index is not a grantable object; access runs
-- through memory_runtime's existing table grants from slot 010.
CREATE INDEX IF NOT EXISTS idx_memory_entries_fleet_id_created_at_key
    ON memory.memory_entries (fleet_id, created_at DESC, key DESC);
