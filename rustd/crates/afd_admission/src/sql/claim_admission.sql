WITH target AS (
    SELECT id
    FROM fleet.admissions
    WHERE producer = $1 AND producer_key = $2
), claimed AS (
    UPDATE fleet.admissions a
    SET replay_claim_expires_at = statement_timestamp() + $3::interval, updated_at = now()
    FROM target t
    WHERE a.id = t.id
      AND a.receipt IS NULL
      AND (a.replay_claim_expires_at IS NULL OR a.replay_claim_expires_at <= statement_timestamp())
    RETURNING a.id
)
SELECT a.id::text AS id, a.fleet_id::text AS fleet_id, a.workspace_id::text AS workspace_id,
       a.actor, a.event_type, a.request_json, a.event_created_at, a.receipt,
       EXISTS (SELECT 1 FROM claimed) AS claimed
FROM fleet.admissions a
JOIN target t ON t.id = a.id
