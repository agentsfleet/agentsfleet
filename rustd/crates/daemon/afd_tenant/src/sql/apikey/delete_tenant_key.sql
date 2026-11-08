WITH current_row AS (
    SELECT id, revoked_at
    FROM identity.api_keys
    WHERE id = $1::uuid AND tenant_id = $2::uuid
), deleted AS (
    DELETE FROM identity.api_keys k
    USING current_row c
    WHERE k.id = c.id AND c.revoked_at IS NOT NULL
    RETURNING k.id
)
SELECT TRUE AS changed
FROM deleted d
UNION ALL
SELECT FALSE AS changed
FROM current_row c
WHERE NOT EXISTS (SELECT 1 FROM deleted)
LIMIT 1
