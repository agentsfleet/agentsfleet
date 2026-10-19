WITH due AS (
    SELECT id
    FROM fleet.admissions
    WHERE receipt IS NULL
      AND created_at <= statement_timestamp() - $1::interval
      AND (replay_claim_expires_at IS NULL OR replay_claim_expires_at <= statement_timestamp())
    ORDER BY created_at ASC, sequence ASC
    LIMIT $2
    FOR UPDATE SKIP LOCKED
), claimed AS (
    UPDATE fleet.admissions a
    SET replay_claim_expires_at = statement_timestamp() + $3::interval, updated_at = now()
    FROM due
    WHERE a.id = due.id
    RETURNING a.id, a.fleet_id, a.workspace_id, a.actor, a.event_type, a.request_json,
              a.event_created_at, a.created_at, a.sequence
)
SELECT id::text AS id, fleet_id::text AS fleet_id, workspace_id::text AS workspace_id,
       actor, event_type, request_json, event_created_at
FROM claimed
ORDER BY created_at ASC, sequence ASC
