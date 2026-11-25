INSERT INTO connector.integration_grants
  (id, fleet_id, service, grant_type, status, scope, requested_reason, approved_at, revoked_at,
   created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7, $8::timestamptz, $9::timestamptz,
        coalesce($10::timestamptz, now()), coalesce($10::timestamptz, now()))
