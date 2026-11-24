-- One coarse check per pass and replica, never one query per idle worker.
-- `pending` answers whether an assignment pass may help: a false positive costs a
-- bounded pass, and a negative must not hide work. Assignments still decide tags,
-- policy, gates and capacity. EXISTS stops at the first match.
-- `idle` names the tenant runners among $1 whose tenant has nothing waiting,
-- read only when something waits. A tenant runner takes only its tenant's
-- work (`select_ledger_candidates.sql`), so an assignment on one of these cannot
-- run. A platform runner is never idle. Each tenant's check is the
-- `pending` check narrowed to that tenant, and stops at its first match.
--
-- $1 the probed runners · $2 whether a checkpointed turn may be reassigned
WITH probe AS (
    SELECT EXISTS (
        -- An admitted fleet turn with no live run on its thread.
        SELECT 1 FROM thread.turns a
        WHERE a.settled_at IS NULL AND a.producer IS NOT NULL
          AND ($2::boolean OR NOT EXISTS (
              SELECT 1 FROM thread.turns saved
              WHERE saved.thread_id = a.thread_id AND saved.sequence <= a.sequence
                AND saved.settled_at IS NULL AND saved.durable_task_id IS NULL
                AND saved.durable_task_generation IS NULL
                AND saved.head_checkpoint_id IS NOT NULL
          ))
          AND NOT EXISTS (
              SELECT 1 FROM thread.threads s
              WHERE s.fleet_id = a.thread_id AND s.run_expires_at > statement_timestamp()
          )
    ) AS pending
)
SELECT
    probe.pending,
    CASE WHEN probe.pending THEN ARRAY(
        SELECT r.id FROM runner.runners r
        WHERE r.id = ANY($1) AND r.tenant_id IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM fleet.fleets f
              JOIN thread.turns a ON a.thread_id = f.id
              WHERE f.tenant_id = r.tenant_id AND a.settled_at IS NULL
              AND ($2::boolean OR NOT EXISTS (
                  SELECT 1 FROM thread.turns saved
                  WHERE saved.thread_id = a.thread_id AND saved.sequence <= a.sequence
                    AND saved.settled_at IS NULL AND saved.durable_task_id IS NULL
                    AND saved.durable_task_generation IS NULL
                    AND saved.head_checkpoint_id IS NOT NULL
              ))
                AND NOT EXISTS (
                    SELECT 1 FROM thread.threads s
                    WHERE s.fleet_id = a.thread_id AND s.run_expires_at > statement_timestamp()
                )
          )
        ORDER BY r.id
    ) ELSE ARRAY[]::uuid[] END AS idle
FROM probe
