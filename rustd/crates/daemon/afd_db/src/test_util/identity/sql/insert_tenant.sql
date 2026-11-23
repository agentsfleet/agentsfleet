INSERT INTO identity.tenants (id, name, created_at)
VALUES ($1, $2, coalesce($3::timestamptz, now()))
