-- 034_retire_redundant_indexes.sql — remove three indexes nothing needs.
--
-- Destructive, so owner-approved per change (SCHEMA_CONVENTIONS "Destructive
-- changes still require an explicit owner decision"). Each drop carries recorded
-- evidence or a structural argument rather than reasoning alone; the counts
-- below were measured under a workload exercising the real handler paths.
--
-- A third candidate, idx_memory_entries_category, was NOT dropped: it recorded
-- 4 scans, so it fails the zero-scan bar, and at 280 kB its write cost does not
-- justify overriding that.
--
-- These indexes are defined in shipped slots 010, 012 and 015, which are frozen
-- history — hence a new slot rather than an edit to those files.
--
-- Both drops are SCHEMA-QUALIFIED, and that is load-bearing rather than style.
-- DROP INDEX resolves against search_path, which does not carry `memory`, and
-- `IF EXISTS` turns a failed resolution into a silent success — so an
-- unqualified drop here reports OK having removed nothing. Qualification is
-- what makes this migration actually do its job. (RULE NSQ.)

-- Superseded by api_keys_hash_uniq, which is UNIQUE on the same column.
--
-- Recorded 0 scans while the unique index took 20 000: the authentication
-- lookup filters key_hash alone and never pairs it with `active`, so the
-- partial index has no query to serve. At 1384 kB it was the largest of the
-- three candidates, on a table every authenticated request writes to via
-- last_used_at.
DROP INDEX IF EXISTS core.idx_api_keys_key_hash_active;

-- Superseded by idx_memory_entries_fleet_id_updated_at_id (slot 033), which
-- leads with the same column.
--
-- This one recorded 57 scans, so it is dropped on structure rather than on
-- disuse: a btree on (fleet_id, updated_at, id) serves every query a btree on
-- (fleet_id) can, because the leading column is identical. Those 57 scans
-- relocate to the composite; they do not become sequential scans.
--
-- Dropping it is also what makes the composite earn its place. While both
-- existed the planner preferred the narrower index for every one of these
-- reads, leaving the composite at 0 scans — an index paying write cost and
-- returning nothing, which is the exact anti-pattern slot 033 set out to fix.
DROP INDEX IF EXISTS memory.idx_memory_entries_fleet_id;

-- Superseded by idx_fleet_events_workspace_id_created_at_event_id (slot 033),
-- which leads with the same two columns and adds the keyset tiebreak.
--
-- Dropped on structure, like the memory index above: a btree on
-- (workspace_id, created_at, event_id) answers everything a btree on
-- (workspace_id, created_at) can, because the leading columns are identical.
-- Keeping both would maintain two near-identical indexes on core.fleet_events —
-- the highest-insert table in the system, one row per event forever — so this is
-- the drop with the largest ongoing write saving of the three.
DROP INDEX IF EXISTS core.idx_fleet_events_workspace_id_created_at;
