INSERT INTO connector.integration_grants
  (id, fleet_id, service, status, requested_reason, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, now(), now())
ON CONFLICT (fleet_id, service) DO NOTHING
