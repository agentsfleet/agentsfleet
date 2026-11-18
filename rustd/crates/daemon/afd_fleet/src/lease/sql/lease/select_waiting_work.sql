-- One coarse check per pass and replica, never one query per idle worker.
-- `pending` answers whether a claim pass may help: a false positive costs a
-- bounded pass, and a negative must not hide work. Claims still decide tags,
-- policy, gates and capacity. EXISTS stops at the first match.
-- `idle` names the tenant runners among $4 whose tenant has nothing waiting,
-- read only when something waits. A tenant runner takes only its tenant's
-- work (`select_ledger_candidates.sql`), so a claim on one of these cannot
-- lease. A platform runner is never idle. Each tenant's check is the
-- `pending` check narrowed to that tenant, and stops at its first match.
WITH probe AS (
    SELECT (
        EXISTS (
            SELECT 1 FROM fleet.admissions a
            WHERE a.settled_at IS NULL
              AND NOT EXISTS (
                  SELECT 1 FROM runner.fleet_slots s
                  WHERE s.fleet_id = a.fleet_id AND s.lease_expires_at > statement_timestamp()
              )
        ) OR EXISTS (
            SELECT 1 FROM authoring.drafts d
            JOIN authoring.messages m ON m.draft_id = d.id
            WHERE d.status = $2 AND m.status = $3 AND m.sequence > d.cancelled_through_sequence
              AND NOT EXISTS (
                  SELECT 1 FROM authoring.attempts t
                  WHERE t.draft_id = d.id AND t.status = $1
                    AND t.lease_expires_at > statement_timestamp() AND t.expires_at > statement_timestamp()
              )
        )
    ) AS pending
)
SELECT
    probe.pending,
    CASE WHEN probe.pending THEN ARRAY(
        SELECT r.id FROM runner.runners r
        WHERE r.id = ANY($4) AND r.tenant_id IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM fleet.fleets f
              JOIN fleet.admissions a ON a.fleet_id = f.id
              WHERE f.tenant_id = r.tenant_id AND a.settled_at IS NULL
                AND NOT EXISTS (
                    SELECT 1 FROM runner.fleet_slots s
                    WHERE s.fleet_id = a.fleet_id AND s.lease_expires_at > statement_timestamp()
                )
          )
          AND NOT EXISTS (
              SELECT 1 FROM authoring.drafts d
              JOIN authoring.messages m ON m.draft_id = d.id
              WHERE d.tenant_id = r.tenant_id
                AND d.status = $2 AND m.status = $3 AND m.sequence > d.cancelled_through_sequence
                AND NOT EXISTS (
                    SELECT 1 FROM authoring.attempts t
                    WHERE t.draft_id = d.id AND t.status = $1
                      AND t.lease_expires_at > statement_timestamp() AND t.expires_at > statement_timestamp()
                )
          )
        ORDER BY r.id
    ) ELSE ARRAY[]::uuid[] END AS idle
FROM probe
