INSERT INTO identity.users (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, now(), now())
