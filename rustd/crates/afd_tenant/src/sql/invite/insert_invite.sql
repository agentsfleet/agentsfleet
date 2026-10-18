INSERT INTO identity.invites (id, tenant_id, email, role, status, invited_by_user_id, expires_at, email_attempt_count, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6::uuid, clock_timestamp() + $7::interval, 0, now(), now())
RETURNING expires_at, created_at
