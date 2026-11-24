SELECT id, service, kind, status, scope::text AS scope,
       requested_scope::text AS requested_scope, created_at, approved_at, revoked_at,
       requested_reason AS reason
FROM connector.integration_grants
WHERE fleet_id = $1
ORDER BY created_at DESC, id DESC
