INSERT INTO identity.user_preferences (id, user_id, workspace_id, name, value, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, now(), now())
ON CONFLICT (user_id, workspace_id, name) DO UPDATE SET
    value = EXCLUDED.value,
    updated_at = now()
