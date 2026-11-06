WITH decisions AS (
    DELETE FROM runner.decision_calls WHERE tenant_id = $1::uuid
), charges AS (
    DELETE FROM billing.usage_lines WHERE tenant_id = $1::uuid
)
DELETE FROM identity.tenants WHERE id = $1::uuid
