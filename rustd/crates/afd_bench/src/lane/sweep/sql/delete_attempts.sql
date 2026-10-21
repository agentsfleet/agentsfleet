DELETE FROM authoring.attempts
 WHERE tenant_id IN (SELECT id FROM identity.tenants WHERE name LIKE $1)
