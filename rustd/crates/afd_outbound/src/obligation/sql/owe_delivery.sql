INSERT INTO fleet.obligations
  (id, fleet_id, workspace_id, provider, destination, event_id, answer,
   receipt, delivered_at, attempt_count, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::text, $7::text, $5::text, $6::text,
        NULL, NULL, 0, now(), now())
ON CONFLICT ON CONSTRAINT uq_obligations_fleet_id_event_id DO NOTHING
RETURNING id
