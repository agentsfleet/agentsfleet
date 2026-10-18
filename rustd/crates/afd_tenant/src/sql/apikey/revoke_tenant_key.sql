WITH current_row AS (
    SELECT id, revoked_at
    FROM identity.api_keys
    WHERE id = $1::uuid AND tenant_id = $2::uuid
), updated AS (
    UPDATE identity.api_keys k
    SET revoked_at = now(), updated_at = now()
    FROM current_row c
    WHERE k.id = c.id AND c.revoked_at IS NULL
    RETURNING k.revoked_at
)
SELECT u.revoked_at, TRUE AS changed
FROM updated u
UNION ALL
SELECT c.revoked_at, FALSE AS changed
FROM current_row c
WHERE NOT EXISTS (SELECT 1 FROM updated)
LIMIT 1
