-- 033_hot_path_indexes.sql — index the recurring control-plane reads.
--
-- Every index below is justified by ONE named production query that today scans
-- or sorts without it. The query is named in the comment above each index; if a
-- query moves or dies, its index is removable and the comment says which to grep.
--
-- Two of these are pure background cost -- the liveness sweep pays them every
-- cycle with no user waiting -- which is what makes this slot worth its write
-- overhead rather than a micro-optimisation.
--
-- Plain CREATE INDEX, not CONCURRENTLY: the migration runner wraps each slot in
-- BEGIN/COMMIT (src/agentsfleetd/db/pool_migrations.zig), and PostgreSQL rejects
-- CONCURRENTLY inside a transaction block. Each build therefore takes a ShareLock
-- for its duration. These tables are small in the deployments this ships to; on a
-- large one the operator builds them by hand outside the migration first, and the
-- IF NOT EXISTS guards make this slot a no-op afterwards.

-- fleet.liveness_sweeper.expireActiveLeaseSlots — filters runner_affinity by
-- last_runner_id once PER DUE RUNNER PER SWEEP CYCLE, so its cost is fleets x
-- runners x cycles. last_runner_id is also an unindexed FK (ON DELETE SET NULL),
-- so deleting a runner scanned this table too. leased_until carries the range
-- predicate in the same statement.
CREATE INDEX IF NOT EXISTS idx_runner_affinity_last_runner_id_leased_until
    ON fleet.runner_affinity (last_runner_id, leased_until);

-- fleet.liveness_sweeper.fetchDueRunners — ORDER BY r.updated_at ASC, r.id ASC
-- LIMIT n, every cycle. fleet.runners carried no index beyond its identity and
-- token-hash uniqueness, so this top-N sorted the whole filtered set each pass.
CREATE INDEX IF NOT EXISTS idx_runners_updated_at_id
    ON fleet.runners (updated_at, id);

-- fleet.reclaim.reclaimPriorActive — WHERE fleet_id = $1 AND status = $2
-- ORDER BY fencing_token DESC LIMIT 1. fleet_id is an unindexed FK (ON DELETE
-- CASCADE), so every reclaim AND every fleet delete scanned runner_leases. The
-- trailing fencing_token makes the whole lookup one seek instead of seek-then-sort.
CREATE INDEX IF NOT EXISTS idx_runner_leases_fleet_id_status_fencing_token
    ON fleet.runner_leases (fleet_id, status, fencing_token DESC);

-- memory.fleet_memory.listAll — the hydration read: WHERE fleet_id = $1
-- ORDER BY updated_at DESC, id DESC. The pre-existing single-column index on
-- fleet_id served the filter but left the sort, so every hydration sorted the
-- fleet's full memory set. This index is a strict superset of that one.
CREATE INDEX IF NOT EXISTS idx_memory_entries_fleet_id_updated_at_id
    ON memory.memory_entries (fleet_id, updated_at DESC, id DESC);

-- state.fleet_events — the workspace keyset page. idx_fleet_events_workspace_id_created_at
-- stops at created_at, so the (created_at = $2 AND event_id < $3) tiebreak became a
-- post-filter on every page. Mirrors the fleet-scoped index in slot 015, which
-- already carries event_id for exactly this reason.
CREATE INDEX IF NOT EXISTS idx_fleet_events_workspace_id_created_at_event_id
    ON core.fleet_events (workspace_id, created_at DESC, event_id DESC);

-- http.handlers.fleets.sql.SELECT_FLEET_PAGE_FIRST / _AFTER — the fleet list page:
-- WHERE workspace_id = $1 ORDER BY created_at DESC, id DESC. The pre-existing
-- idx_fleets_workspace_id_created_at_active cannot serve it: that index is PARTIAL
-- on status='active' and the list is not status-filtered, and it lacks the id
-- tiebreak the keyset cursor pages on.
CREATE INDEX IF NOT EXISTS idx_fleets_workspace_id_created_at_id
    ON core.fleets (workspace_id, created_at DESC, id DESC);

-- http.handlers.api_keys.list — both created_at sorts. The tenant_id equality leads,
-- so one btree serves ASC (forward) and DESC (backward); uid is the tiebreak the
-- sort clause actually names. idx_api_keys_tenant_active covers (tenant_id, active)
-- and serves neither ordering.
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_id_created_at_uid
    ON core.api_keys (tenant_id, created_at DESC, uid DESC);

-- http.handlers.api_keys.list — both key_name sorts, same reasoning.
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_id_key_name_uid
    ON core.api_keys (tenant_id, key_name, uid);

-- http.handlers.fleet.runners_list — the default sort and its ascending twin
-- (r.created_at DESC, r.id DESC / ASC, ASC) over the unfiltered runner set.
CREATE INDEX IF NOT EXISTS idx_runners_created_at_id
    ON fleet.runners (created_at DESC, id DESC);

-- http.handlers.fleet.runners_list — the host_id sorts, the remaining pair from
-- the same allowlist.
CREATE INDEX IF NOT EXISTS idx_runners_host_id_id
    ON fleet.runners (host_id, id);
