INSERT INTO memory.context_checkpoints
    (id, draft_id, lease_id, fleet_id, branch_id, resource_version, document, created_at, updated_at)
SELECT $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, $7, now(), now()
 WHERE $8::bigint = 0
ON CONFLICT DO NOTHING
