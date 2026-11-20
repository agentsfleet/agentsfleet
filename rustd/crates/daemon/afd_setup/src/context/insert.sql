INSERT INTO memory.context_checkpoints
    (id, draft_id, resource_version, document, created_at, updated_at)
SELECT $1, $2, $3, $4, now(), now()
 WHERE $5::bigint = 0
ON CONFLICT DO NOTHING
