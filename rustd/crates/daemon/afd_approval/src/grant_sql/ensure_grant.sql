INSERT INTO connector.integration_grants
  (id, fleet_id, service, grant_type, status, requested_reason, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, now(), now())
ON CONFLICT (fleet_id, grant_type, service) DO NOTHING
