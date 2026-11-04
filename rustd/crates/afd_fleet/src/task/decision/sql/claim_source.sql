UPDATE fleet.admissions a
SET input_claim_lease_id = $2::uuid, input_claim_fence = $3, updated_at = clock_timestamp()
WHERE a.id = $1::uuid
  AND (
      a.input_claim_lease_id IS NULL
      OR (a.input_claim_lease_id = $2::uuid AND a.input_claim_fence = $3)
      OR NOT EXISTS (
          SELECT 1 FROM runner.leases l
          JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id
          WHERE l.id = a.input_claim_lease_id AND l.fencing_token = a.input_claim_fence
            AND l.status = $4 AND l.lease_expires_at > clock_timestamp()
            AND s.fencing_token = l.fencing_token AND s.last_runner_id = l.runner_id
      )
  )
  AND NOT EXISTS (
      SELECT 1 FROM fleet.admissions earlier
      WHERE earlier.fleet_id = a.fleet_id AND earlier.workspace_id = a.workspace_id
        AND earlier.producer = $5 AND earlier.actor <> $6
        AND earlier.settled_at IS NULL AND earlier.id <> $7::uuid
        AND (earlier.created_at, earlier.id) < (a.created_at, a.id)
  )
RETURNING id::text
