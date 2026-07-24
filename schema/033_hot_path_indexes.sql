-- 033_hot_path_indexes.sql — index the reads whose cost grows without bound.
--
-- Scale assumption: ~100 runners. That number is load-bearing here, because it
-- decides which reads are worth an index and which are not.
--
-- The four indexes below all guard tables that grow with USAGE OVER TIME —
-- events accumulate per execution, leases per claim, memory per fleet, affinity
-- per fleet. Those keep growing at 100 runners just as they would at 10,000, so
-- the read cost climbs with the account's age unless an index bounds it.
--
-- Deliberately NOT indexed: `fleet.runners`, `core.fleets` and `core.api_keys`
-- list sorts. Those tables grow when a person adds a runner, a fleet, or a key —
-- roughly a hundred rows, which fits in a page or two. Sorting a hundred rows is
-- already free, and the sweep's own LIMIT (100) is the whole table at that size,
-- so an index there would buy nothing while adding a thing to maintain and
-- reason about. Revisit if runner or fleet counts reach the low thousands; the
-- queries are unchanged, so adding an index later is a one-line slot.
--
-- Plain CREATE INDEX, not CONCURRENTLY: the migration runner wraps each slot in
-- BEGIN/COMMIT (src/agentsfleetd/db/pool_migrations.zig), and PostgreSQL rejects
-- CONCURRENTLY inside a transaction block. Each build takes a ShareLock for its
-- duration. On a populated deployment the operator builds these by hand outside
-- the migration first; the IF NOT EXISTS guards make this slot a no-op after.

-- fleet.liveness_sweeper.expireActiveLeaseSlots — filters runner_affinity by
-- last_runner_id once PER DUE RUNNER PER SWEEP CYCLE, so its cost is fleets x
-- runners x cycles: the only multiplicative read in the sweep, and the table
-- grows with fleet count independently of the runner assumption above.
-- last_runner_id is also an unindexed FK (ON DELETE SET NULL), so deleting a
-- runner scanned this table too. leased_until carries the range predicate in the
-- same statement.
CREATE INDEX IF NOT EXISTS idx_runner_affinity_last_runner_id_leased_until
    ON fleet.runner_affinity (last_runner_id, leased_until);

-- fleet.reclaim.reclaimPriorActive — WHERE fleet_id = $1 AND status = $2
-- ORDER BY fencing_token DESC LIMIT 1. runner_leases gains a row per claim and
-- is never pruned, so this is unbounded growth. fleet_id is an unindexed FK
-- (ON DELETE CASCADE), so every reclaim AND every fleet delete scanned it. The
-- trailing fencing_token makes the whole lookup one seek instead of seek-then-sort.
CREATE INDEX IF NOT EXISTS idx_runner_leases_fleet_id_status_fencing_token
    ON fleet.runner_leases (fleet_id, status, fencing_token DESC);

-- state.fleet_events — the workspace keyset page, over the table with the
-- highest insert rate in the system: one row per event, forever.
-- idx_fleet_events_workspace_id_created_at stops at created_at, so the
-- (created_at = $2 AND event_id < $3) tiebreak became a post-filter on every
-- page. Mirrors the fleet-scoped index in slot 015, which already carries
-- event_id for exactly this reason. Slot 034 retires the two-column index this
-- one supersedes, so the table's index count is unchanged.
CREATE INDEX IF NOT EXISTS idx_fleet_events_workspace_id_created_at_event_id
    ON core.fleet_events (workspace_id, created_at DESC, event_id DESC);

-- memory.fleet_memory — a fleet's memory set, filtered by fleet_id and read
-- newest-first. Slot 034 retires the single-column fleet_id index this one
-- supersedes as a strict prefix, so this becomes the only index serving the
-- filter; without it those reads would fall back to a sequential scan.
CREATE INDEX IF NOT EXISTS idx_memory_entries_fleet_id_updated_at_id
    ON memory.memory_entries (fleet_id, updated_at DESC, id DESC);
