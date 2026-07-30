-- Index support for the operator-plane lease read. Two costs the read pays per
-- page, neither served by an existing index:
--
--   1. The page itself seeks WHERE runner_id = $1 ORDER BY created_at DESC,
--      id DESC. Slot 018's (runner_id, status) answers the liveness EXISTS but
--      carries no created_at, so the page fetched the runner's whole lease
--      history and sorted it to return one bounded page.
--   2. Each returned row derives is_reclaim by probing for a lower-fencing
--      sibling on (fleet_id, event_id). Slot 033's
--      (fleet_id, status, fencing_token DESC) is usable on its fleet_id prefix
--      only and does not carry event_id, so the probe visited the heap for
--      every lease the fleet ever issued — once per returned row.
--
-- Both grow without bound: runner_leases gains a row per claim and is never
-- pruned (slot 033's own note). Measured on one runner holding 5,000 leases
-- across 5 fleets (the spec's motivating runner holds 4,021): the page read goes
-- 15.94 ms -> 0.405 ms, the full-history Seq Scan plus top-N heapsort becomes a
-- 25-row Index Scan with no sort node, and the reclaim probe becomes an Index
-- Only Scan instead of 997 index entries plus heap visits per returned row. The
-- composite is also the index the windowed-counters follow-up needs.
--
-- Not addressed here: the list's COUNT(*) WHERE runner_id still plans as a Seq
-- Scan (1.1 ms, unchanged), which is correct at this size — the runner's history
-- is most of the table. It becomes a candidate only once many runners share it.
--
-- Additive-only: two indexes, no table or column change, no row rewrite.
-- RULE SGR: no GRANT lines — an index is not a grantable object; access runs
-- through api_runtime's existing table grants from slot 018.
CREATE INDEX IF NOT EXISTS idx_runner_leases_runner_id_created_at_id
    ON fleet.runner_leases (runner_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_runner_leases_fleet_id_event_id_fencing_token
    ON fleet.runner_leases (fleet_id, event_id, fencing_token);
