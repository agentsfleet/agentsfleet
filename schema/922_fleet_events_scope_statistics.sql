-- Additive migration: tell the planner that a fleet event's workspace follows
-- from its fleet.
--
-- Readers served: the event history's fleet-scoped reads, which all filter
-- `workspace_id = $1 AND fleet_id = $2` and page newest-first on
-- `idx_fleet_events_fleet_id_created_at_event_id` (schema/800) — the fleet's
-- history page, the workspace listing narrowed to one fleet, and the fleet's
-- chat thread (`afd_events::history::statement`).
--
-- Why: every fleet belongs to exactly one workspace, but without this object
-- the planner treats the two equalities as independent and multiplies their
-- selectivities. It then expects about one matching row, and for a cached,
-- generic plan it chooses a bitmap scan plus a sort, which reads every row the
-- fleet has (above the cursor and `since`) to serve one page. With the
-- functional dependency known, the estimate is the fleet's own row count, and
-- the plan walks the fleet index in order until `LIMIT` stops it.
--
-- The object holds nothing until the table is analysed, so the slot analyses
-- it once; autovacuum keeps it current after that. ANALYZE samples a bounded
-- number of rows, so its cost does not grow with the table.
--
-- No table, column or grant changes: a statistics object is read only by the
-- planner. VERSION is 0.51.0, above the 0.30.0 anchor, so schema/800 stays
-- frozen history. Idempotent (IF NOT EXISTS) for a fresh bootstrap and an
-- already-provisioned database alike.

CREATE STATISTICS IF NOT EXISTS core.stx_fleet_events_workspace_id_fleet_id (dependencies)
    ON workspace_id, fleet_id FROM core.fleet_events;

ANALYZE core.fleet_events;
