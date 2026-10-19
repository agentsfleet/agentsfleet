SELECT tenant_id::text AS tenant, created_by_subject AS subject, revoked_at IS NULL AS usable
FROM identity.api_keys
WHERE key_hash = $1
LIMIT 1
