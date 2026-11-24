INSERT INTO connector.integration_grants
  (id, fleet_id, service, kind, status, scope, requested_scope, requested_reason, approved_at,
   revoked_at, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7::jsonb, $8, $9::timestamptz, $10::timestamptz,
        coalesce($11::timestamptz, now()), coalesce($11::timestamptz, now()))
