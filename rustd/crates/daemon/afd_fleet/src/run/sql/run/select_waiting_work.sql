-- One coarse check per pass and replica, never one query per idle worker.
-- `pending` answers whether an assignment pass may help: a false positive costs a
-- bounded pass, and a negative must not hide work. Assignments still decide tags,
-- policy, gates and capacity. EXISTS stops at the first match.
-- `idle` names the tenant runners among $4 whose tenant has nothing waiting,
-- read only when something waits. A tenant runner takes only its tenant's
-- work (`select_ledger_candidates.sql`), so an assignment on one of these cannot
-- run. A platform runner is never idle. Each tenant's check is the
-- `pending` check narrowed to that tenant, and stops at its first match.
WITH probe AS (
    SELECT (
        EXISTS (
            -- An admitted fleet turn: a setup draft's turn has no producer.
            SELECT 1 FROM thread.turns a
            WHERE a.settled_at IS NULL AND a.producer IS NOT NULL
              AND ($5::boolean OR NOT EXISTS (
                  SELECT 1 FROM thread.turns saved
                  JOIN runner.turn_recovery recovery ON recovery.turn_id = saved.id
                  WHERE saved.thread_id = a.thread_id AND saved.sequence <= a.sequence
                    AND saved.settled_at IS NULL AND saved.task_id IS NULL
                    AND saved.task_generation IS NULL
                    AND recovery.head_turn_checkpoint_id IS NOT NULL
              ))
              AND NOT EXISTS (
                  SELECT 1 FROM thread.threads s
                  WHERE s.fleet_id = a.thread_id AND s.run_expires_at > statement_timestamp()
              )
        ) OR EXISTS (
            -- A setup draft waiting for a turn: a queued message, or a running one a lapsed
            -- draft run left behind (its run's `turn_id` is the message), with neither a
            -- live attempt nor a live draft thread holding the draft.
            SELECT 1 FROM setup.drafts d
            JOIN thread.turns m ON m.thread_id = d.id
            WHERE d.status = $2 AND m.settled_at IS NULL
              AND (m.status = $3
                   OR (m.status = $6 AND EXISTS (
                       SELECT 1 FROM thread.runs l
                       WHERE l.turn_id = m.id AND l.draft_id = d.id)))
              -- Setup turns the authoring path still runs; this term goes with setup.attempts.
              AND NOT EXISTS (
                  SELECT 1 FROM setup.attempts t
                  WHERE t.draft_id = d.id AND t.status = $1
                    AND t.run_expires_at > statement_timestamp() AND t.expires_at > statement_timestamp()
              )
              AND NOT EXISTS (
                  SELECT 1 FROM thread.threads s
                  WHERE s.id = d.id AND s.run_expires_at > statement_timestamp()
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
              JOIN thread.turns a ON a.thread_id = f.id
              WHERE f.tenant_id = r.tenant_id AND a.settled_at IS NULL
              AND ($5::boolean OR NOT EXISTS (
                  SELECT 1 FROM thread.turns saved
                  JOIN runner.turn_recovery recovery ON recovery.turn_id = saved.id
                  WHERE saved.thread_id = a.thread_id AND saved.sequence <= a.sequence
                    AND saved.settled_at IS NULL AND saved.task_id IS NULL
                    AND saved.task_generation IS NULL
                    AND recovery.head_turn_checkpoint_id IS NOT NULL
              ))
                AND NOT EXISTS (
                    SELECT 1 FROM thread.threads s
                    WHERE s.fleet_id = a.thread_id AND s.run_expires_at > statement_timestamp()
                )
          )
          -- The setup branch above, narrowed to the runner's tenant, and re-pointed with it.
          AND NOT EXISTS (
              SELECT 1 FROM setup.drafts d
              JOIN thread.turns m ON m.thread_id = d.id
              WHERE d.tenant_id = r.tenant_id
                AND d.status = $2 AND m.settled_at IS NULL
                AND (m.status = $3
                     OR (m.status = $6 AND EXISTS (
                         SELECT 1 FROM thread.runs l
                         WHERE l.turn_id = m.id AND l.draft_id = d.id)))
                AND NOT EXISTS (
                    SELECT 1 FROM setup.attempts t
                    WHERE t.draft_id = d.id AND t.status = $1
                      AND t.run_expires_at > statement_timestamp() AND t.expires_at > statement_timestamp()
                )
                AND NOT EXISTS (
                    SELECT 1 FROM thread.threads s
                    WHERE s.id = d.id AND s.run_expires_at > statement_timestamp()
                )
          )
        ORDER BY r.id
    ) ELSE ARRAY[]::uuid[] END AS idle
FROM probe
