INSERT INTO memory.context_checkpoints
    (id, draft_id, lease_id, fleet_id, branch_id, resource_version, document, created_at, updated_at)
SELECT $1, $2, $3, $4, $5, $6, $7, now(), now()
 WHERE $8::bigint = 0
ON CONFLICT DO NOTHING
