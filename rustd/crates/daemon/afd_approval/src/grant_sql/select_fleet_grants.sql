SELECT id, service, status, created_at, approved_at, revoked_at,
       requested_reason AS reason
FROM connector.integration_grants
WHERE fleet_id = $1
ORDER BY created_at DESC, id DESC
