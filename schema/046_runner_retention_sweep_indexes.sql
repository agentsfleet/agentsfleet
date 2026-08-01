-- Index support for the runner retention sweep (fleet/retention_sweeper.zig).
--
-- Both sweep DELETEs searched by sequential scan, hourly, on every replica.
-- Neither table carried an index leading with the sweep's own predicate:
-- slot 018's is (runner_id, status), slot 041's leads with runner_id, and
-- slots 021/044 lead with runner_id too. The sweeper filters by status/tag and
-- age across ALL runners, so a runner-leading index cannot serve it.
--
-- Worse in steady state than during the initial backlog: once the backlog
-- drains, the LIMIT can never short-circuit, so each cycle scans the whole
-- table to prove that fewer than the batch limit of rows qualify. Three
-- production machines each run their own sweeper, so that cost multiplies and
-- the scans contend.
--
-- Measured on the steady-state cycle (the one that finds nothing), EXPLAIN
-- ANALYZE, Jul 31 2026:
--
--   leases, 50,000 rows:            Seq Scan on runner_leases  37.9 ms
--     -> Index Scan using idx_runner_leases_status_updated_at   0.56 ms
--   events, 100,000 rows / 201 runners:
--                Index Scan using runner_events_runner_idx      4.76 ms
--     -> Index Scan using idx_runner_events_type_occurred_at    0.36 ms
--
-- The events number is why the runner-leading index does not already cover
-- this: it can only bound `occurred_at` WITHIN one runner's segment, so the
-- sweep walks every runner's segment in turn. On a single-runner fixture it
-- looks free and the planner even prefers it -- measure at real cardinality or
-- this index reads as redundant.
--
-- Full composites, not partial. The reads bind both value sets as parameter
-- arrays (status = ANY($1), event_type = ANY($1)) and the planner cannot prove
-- a bound parameter satisfies a partial-index predicate, so a generic plan
-- falls off a partial index entirely -- the same measurement that redirected
-- slot 044 (spec Discovery C2). RULE STS: the value sets stay in application
-- constants and arrive as parameters; no literal vocabulary appears here.
--
-- The lease index leads with status because the sweep's status set is two
-- values out of five while its age predicate is a half-open range: equality
-- first, range second, is the order an index scan can use for both.
--
-- Additive-only: two indexes, no table or column change, no row rewrite.
-- RULE SGR: no GRANT lines -- an index is not a grantable object; the sweep's
-- DELETE privilege comes from slot 045.
CREATE INDEX IF NOT EXISTS idx_runner_leases_status_updated_at
    ON fleet.runner_leases (status, updated_at);

CREATE INDEX IF NOT EXISTS idx_runner_events_type_occurred_at
    ON fleet.runner_events (event_type, occurred_at);
