SELECT id::text AS id, service, status, created_at, approved_at, revoked_at,
       requested_reason AS reason
FROM connector.integration_grants
WHERE fleet_id = $1::uuid
ORDER BY created_at DESC, id DESC
