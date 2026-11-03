SELECT actor, producer, request_json, reply_provider, reply_address,
       input_claim_lease_id::text AS input_claim_lease_id, input_claim_fence
FROM fleet.admissions
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid
  AND settled_at IS NULL
FOR UPDATE
