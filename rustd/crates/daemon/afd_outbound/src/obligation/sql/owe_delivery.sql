INSERT INTO fleet.obligations
  (id, fleet_id, workspace_id, provider, destination, event_id, answer,
   receipt, claim_expires_at, delivered_at, attempt_count, created_at, updated_at)
VALUES ($1, $2, $3, $4::text, $7::text, $5, $6::text,
        $8, clock_timestamp(), NULL, 0, now(), now())
ON CONFLICT ON CONSTRAINT uq_obligations_fleet_id_event_id DO NOTHING
RETURNING id
