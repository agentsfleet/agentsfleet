INSERT INTO fleet.sessions (fleet_id, context_json, created_at, updated_at)
VALUES ($1::uuid, $2::jsonb, now(), now())
ON CONFLICT (fleet_id) DO UPDATE
  SET context_json = EXCLUDED.context_json, updated_at = now()
