-- The five indexes on fleet.runner_leases, each with the query it serves.
--
-- They are grouped here rather than beside the CREATE TABLE because each one
-- carries measured evidence, and the table statement stays readable without it.
-- RULE SGR does not apply: an index is not a grantable object, and access runs
-- through the table grants in schema/610_runner_leases.sql.
--
-- What every one of these has in common: `fleet.runner_leases` gains a row per
-- claim, so each read below grows with the deployment's age rather than its size.
-- The retention sweep bounds the table at the window, not at a row count.

-- Reader: the liveness sweeper's slot-expiry pass, which enumerates one runner's
-- active lease set (expireActiveLeaseSlots).
CREATE INDEX IF NOT EXISTS idx_runner_leases_runner_id_status
    ON fleet.runner_leases (runner_id, status);

-- Reader: fleet/reclaim.zig reclaimPriorActive —
-- WHERE fleet_id = $1 AND status = $2 ORDER BY fencing_token DESC LIMIT 1.
-- `fleet_id` is also a cascading foreign key, so before this index every reclaim
-- AND every fleet delete scanned the table. The trailing fencing token makes the
-- whole lookup one seek instead of a seek followed by a sort.
CREATE INDEX IF NOT EXISTS idx_runner_leases_fleet_id_status_fencing_token
    ON fleet.runner_leases (fleet_id, status, fencing_token DESC);

-- Reader: the operator-plane lease page —
-- WHERE runner_id = $1 ORDER BY created_at DESC, id DESC.
-- The (runner_id, status) index above answers the liveness EXISTS but carries no
-- created_at, so without this one the page fetched a runner's whole lease history
-- and sorted it to return a single bounded page. Measured on one runner holding
-- 5,000 leases across 5 fleets: 15.94 ms → 0.405 ms, a full-history sequential
-- scan plus top-N heapsort becoming a 25-row index scan with no sort node.
CREATE INDEX IF NOT EXISTS idx_runner_leases_runner_id_created_at_id
    ON fleet.runner_leases (runner_id, created_at DESC, id DESC);

-- Reader: the is_reclaim derivation on each returned lease row, which probes for
-- a lower-fencing sibling on (fleet_id, event_id). The reclaim index above is
-- usable on its fleet_id prefix only and does not carry event_id, so the probe
-- visited the heap for every lease the fleet had ever issued — once per returned
-- row. With this index it becomes an index-only scan.
CREATE INDEX IF NOT EXISTS idx_runner_leases_fleet_id_event_id_fencing_token
    ON fleet.runner_leases (fleet_id, event_id, fencing_token);

-- Reader: the retention sweep (fleet/retention_sweeper.zig), which deletes
-- terminal-status rows older than the window across ALL runners — so no
-- runner-leading index can serve it, and every one above leads with a runner or
-- a fleet. Measured on the steady-state cycle, the one that finds nothing and
-- therefore cannot short-circuit on its LIMIT: 50,000 rows, sequential scan
-- 37.9 ms → index scan 0.56 ms, once per hour per replica.
--
-- A full composite, not a partial index: the sweep binds its status set as a
-- parameter array (status = ANY($1)), and the planner cannot prove a bound
-- parameter satisfies a partial-index predicate, so a generic plan falls off a
-- partial index entirely. That also keeps the vocabulary in application
-- constants rather than in an index predicate here (RULE STS).
--
-- Status leads because the sweep's status set is two values out of five while
-- its age predicate is a half-open range: equality first, range second, is the
-- order one index scan can use for both.
CREATE INDEX IF NOT EXISTS idx_runner_leases_status_updated_at
    ON fleet.runner_leases (status, updated_at);
