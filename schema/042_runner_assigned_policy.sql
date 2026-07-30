-- Assigned runner policy + reported capability on fleet.runners.
--
-- Direction of authority: the control plane ASSIGNS policy to a runner row and
-- delivers it with the runner's identity; the runner reports what its kernel
-- can actually enforce; the heartbeat path reconciles the two into a degraded
-- verdict. Assigned and achievable live in separate columns so no code path
-- can overwrite one with the other.
--
-- sandbox_tier (017) carries the ASSIGNED tier from this migration on: the
-- register handler writes the operator's assignment, never a host self-report.
-- Pre-existing rows keep their last recorded value as the initial assignment.
--
-- network_policy / registry_allowlist: assigned values, app-enforced
--   vocabularies (RULE STS: no string CHECK/DEFAULT). NULL = assigned before
--   this migration existed — the reconciliation marks such runners degraded
--   until an operator assigns a policy; the runner side fails closed.
-- worker_count: assigned concurrency for the host's worker pool.
--   Canonical constant: DEFAULT_WORKER_COUNT (src/lib/contract/protocol.zig)
-- capability_report: the runner's latest probe result, verbatim JSON, written
--   only by the heartbeat path. NULL = no report yet (also a degraded state).
-- capability_reported_at: ms epoch of that report's arrival.
-- degraded / degraded_reason: the reconciliation verdict; the reason names the
--   specific missing mechanism, written by the heartbeat path.

ALTER TABLE fleet.runners ADD COLUMN IF NOT EXISTS network_policy TEXT NULL;
ALTER TABLE fleet.runners ADD COLUMN IF NOT EXISTS registry_allowlist JSONB NULL;
ALTER TABLE fleet.runners ADD COLUMN IF NOT EXISTS worker_count INTEGER NOT NULL DEFAULT 1;
ALTER TABLE fleet.runners ADD COLUMN IF NOT EXISTS capability_report JSONB NULL;
ALTER TABLE fleet.runners ADD COLUMN IF NOT EXISTS capability_reported_at BIGINT NULL;
ALTER TABLE fleet.runners ADD COLUMN IF NOT EXISTS degraded BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE fleet.runners ADD COLUMN IF NOT EXISTS degraded_reason TEXT NULL;

-- Backfill: every pre-migration row lacks an assignment (network_policy NULL),
-- so it starts DEGRADED — the lease gate must fail closed from the moment this
-- deploys, not from each runner's first post-deploy heartbeat. The reason
-- string mirrors REASON_NO_ASSIGNED_POLICY (heartbeat_reconcile.zig); the
-- reconciliation rewrites it verbatim on the next beat either way.
UPDATE fleet.runners SET degraded = TRUE, degraded_reason = 'no assigned policy'
WHERE network_policy IS NULL;
