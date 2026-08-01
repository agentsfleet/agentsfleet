-- The indexes on billing.usage_ledger, each with the query it serves.
--
-- The retired table carried four. One of them — a (fleet_id, recorded_at) index
-- for an operator all-fleets list — has no reader left and is gone. The
-- per-fleet (workspace_id, fleet_id, recorded_at) index does have a reader and
-- comes back below, reshaped: the budget drain moved onto this table when
-- `fleet.metering_periods` was deleted, so the fleet-scoped read this slot once
-- called readerless is now the hottest gate in the money path.
--
-- What replaces them is narrower and has a different job: the three identity
-- columns are real foreign keys now, and an unindexed referential action on an
-- unbounded table means every parent delete scans it — the same defect the
-- retired hot-path-index slot found on `runner_leases.fleet_id`. The action
-- differs per column (see schema/710): the tenant cascades, the workspace and
-- the fleet SET NULL, and all three have to find their rows either way.
--
-- RULE SGR does not apply: an index is not a grantable object, and access runs
-- through the grants in schema/710_usage_ledger.sql.

-- Reader: GET /v1/tenants/me/billing/charges (listTelemetryForTenant) —
-- WHERE tenant_id = $1 ORDER BY created_at DESC, id DESC, cursor-paged.
--
-- The trailing `id` is the point (RULE KYS). The retired index stopped at
-- recorded_at, so the keyset's (created_at = $2 AND id < $3) tiebreak became a
-- post-filter on every page: the plan seeked to the timestamp and then sorted to
-- resolve ties. Carrying the tiebreak makes the page one index scan with no sort
-- node, which is asserted against the plan rather than against the index
-- definition.
--
-- Its `tenant_id` prefix also serves the tenant cascade during account erasure.
CREATE INDEX IF NOT EXISTS idx_usage_ledger_tenant_id_created_at_id
    ON billing.usage_ledger (tenant_id, created_at DESC, id DESC);

-- Two readers, one index, and the column ORDER is what lets it serve both.
--
-- Reader 1 — the budget drain (`fleet/sql.zig` SELECT_BUDGET_DRAIN and
-- SELECT_BUDGET_POLICY_AND_DRAIN): WHERE workspace_id = $1 AND fleet_id = $2 AND
-- last_charged_at >= floor. It runs on every event receive and every renewal —
-- roughly every 25 seconds per live run — and this table is never pruned
-- (schema/710 grants no DELETE), so an unindexed form degrades with a fleet's
-- lifetime spend rather than with anything bounded. That is the idle-cost defect
-- `docs/architecture/scaling.md` exists to refuse. Two equalities then a range is
-- exactly the shape one index scan can serve.
--
-- Reader 2 — the fleet SET NULL. Deleting a fleet is routine, not just an
-- erasure step, and it must find and detach that fleet's ledger rows.
--
-- `fleet_id` LEADS for reader 2's sake: a referential action matches on
-- `fleet_id` alone, and a btree leading with `workspace_id` could not serve it —
-- which is why this is one index and not two. Reader 1 is indifferent to the
-- order of the two equality columns.
CREATE INDEX IF NOT EXISTS idx_usage_ledger_fleet_id_workspace_id_last_charged_at
    ON billing.usage_ledger (fleet_id, workspace_id, last_charged_at);

-- Reader: the workspace SET NULL. Same argument as the fleet index above, and
-- kept for the same reason even though deleting a workspace also deletes its
-- fleets: PostgreSQL does not guarantee the fleet action reaches these rows
-- first, so relying on that ordering would leave a sequential scan over an
-- unbounded table on a path meant to be quick.
CREATE INDEX IF NOT EXISTS idx_usage_ledger_workspace_id
    ON billing.usage_ledger (workspace_id);
