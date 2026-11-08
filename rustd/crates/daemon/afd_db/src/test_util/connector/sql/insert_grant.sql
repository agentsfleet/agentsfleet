INSERT INTO connector.integration_grants
  (id, fleet_id, service, status, requested_reason, approved_at, revoked_at,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6::timestamptz, $7::timestamptz,
        coalesce($8::timestamptz, now()), coalesce($8::timestamptz, now()))
