WITH source AS MATERIALIZED (
    SELECT sequence, reply_provider, reply_address
    FROM thread.turns WHERE id = $1 AND thread_id = $2 AND workspace_id = $3
), candidates AS MATERIALIZED (
    (SELECT a.id, a.sequence
     FROM thread.turns a CROSS JOIN source s
     WHERE s.reply_provider IS NOT NULL AND a.thread_id = $2 AND a.workspace_id = $3
       AND a.reply_provider = s.reply_provider AND a.reply_address = s.reply_address
       AND a.settled_at IS NULL AND a.sequence > s.sequence AND a.sequence <= $4
     ORDER BY a.sequence LIMIT $5)
    UNION ALL
    (SELECT a.id, a.sequence
     FROM thread.turns a CROSS JOIN source s
     WHERE s.reply_provider IS NULL AND a.thread_id = $2 AND a.workspace_id = $3
       AND a.reply_provider IS NULL AND a.reply_address IS NULL
       AND a.settled_at IS NULL AND a.sequence > s.sequence AND a.sequence <= $4
     ORDER BY a.sequence LIMIT $5)
), locked AS MATERIALIZED (
    SELECT a.id, a.sequence, a.input_claim_run_id, a.input_claim_fence,
           a.parked_at, a.settled_at, a.task_id, a.task_generation, a.producer, a.actor,
           a.workspace_id
    FROM thread.turns a JOIN candidates c ON c.id = a.id
    ORDER BY a.sequence FOR UPDATE OF a
)
SELECT id, sequence, input_claim_run_id, input_claim_fence,
       parked_at IS NULL AND settled_at IS NULL AND task_id IS NULL AND task_generation IS NULL
       AND CASE WHEN producer = $6 THEN actor <> $7 AND actor LIKE $8
                WHEN producer = $9 THEN COALESCE(
                    connector.slack_person_allowed(workspace_id, actor, $10, $11, $12), false)
                ELSE false END AS foldable
FROM locked ORDER BY sequence
