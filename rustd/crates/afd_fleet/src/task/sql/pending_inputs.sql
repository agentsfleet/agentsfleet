-- Reads and claims share conversation, FIFO and live-owner eligibility.
-- A live claim is a barrier; it must never be skipped in favour of later input.
WITH origin AS MATERIALIZED (
    SELECT reply_provider, reply_address FROM fleet.admissions
    WHERE id = $3::uuid AND fleet_id = $1::uuid AND workspace_id = $2::uuid
), conversation AS (
    SELECT a.id, a.sequence, a.request_json, a.producer, a.actor, a.input_claim_lease_id, a.input_claim_fence
    FROM fleet.admissions a CROSS JOIN origin o
    WHERE a.fleet_id = $1::uuid AND a.workspace_id = $2::uuid
      AND o.reply_provider IS NOT NULL
      AND a.reply_provider = o.reply_provider AND a.reply_address = o.reply_address
      AND a.settled_at IS NULL AND ($13 OR a.interpreted_at IS NULL)
    UNION ALL
    SELECT a.id, a.sequence, a.request_json, a.producer, a.actor, a.input_claim_lease_id, a.input_claim_fence
    FROM fleet.admissions a CROSS JOIN origin o
    WHERE a.fleet_id = $1::uuid AND a.workspace_id = $2::uuid
      AND o.reply_provider IS NULL
      AND a.reply_provider IS NULL AND a.reply_address IS NULL
      AND a.settled_at IS NULL AND ($13 OR a.interpreted_at IS NULL)
), candidates AS MATERIALIZED (
    SELECT a.id, a.sequence, a.request_json,
           (a.input_claim_lease_id IS NULL
            OR (a.input_claim_lease_id = $9::uuid AND a.input_claim_fence = $10)
            OR NOT EXISTS (
                SELECT 1 FROM runner.leases claimed
                JOIN runner.fleet_slots s ON s.fleet_id = claimed.fleet_id
                WHERE claimed.id = a.input_claim_lease_id
                  AND claimed.fencing_token = a.input_claim_fence
                  AND claimed.status = $11 AND claimed.lease_expires_at > clock_timestamp()
                  AND s.last_runner_id = claimed.runner_id AND s.fencing_token = claimed.fencing_token
            )) AS claimable
    FROM conversation a
    WHERE a.producer = $4 AND a.actor <> $5 AND a.actor LIKE $6
      AND a.id <> $3::uuid AND a.sequence > $7
    ORDER BY a.sequence LIMIT $8
), eligible AS MATERIALIZED (
    SELECT id, sequence, request_json FROM candidates
    WHERE claimable AND NOT EXISTS (
        SELECT 1 FROM candidates barrier WHERE NOT barrier.claimable AND barrier.sequence <= candidates.sequence)
), claimed AS (
    UPDATE fleet.admissions a
    SET input_claim_lease_id = $9::uuid, input_claim_fence = $10, updated_at = now()
    FROM eligible e
    WHERE a.id = e.id AND a.id = $12::uuid
      AND e.sequence = (SELECT min(sequence) FROM eligible)
    RETURNING e.id, e.sequence, e.request_json
)
SELECT id::text, sequence, request_json FROM eligible WHERE $12::uuid IS NULL
UNION ALL
SELECT id::text, sequence, request_json FROM claimed
ORDER BY sequence
