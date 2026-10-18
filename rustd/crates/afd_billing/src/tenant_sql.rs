//! Read both durable cost sources that debit the shared wallet. No money moves
//! here. UUIDs stay native through comparisons and sorting, before wire casts.

/// The wallet snapshot behind `GET /v1/tenants/me/billing`.
pub(crate) const SELECT_TENANT_BALANCE: &str = "\
SELECT balance_nanos, updated_at, balance_exhausted_at \
FROM billing.tenant_wallet \
WHERE tenant_id = $1::uuid \
LIMIT 1";

// One projection for both cursor forms. The private draft contributes only its
// public fleet link; no title, message, candidate or credential enters the read.
macro_rules! charges_page {
    ($boundary:literal, $limit:literal) => {
        concat!(
            "WITH charges AS ( \
             SELECT id, tenant_id, workspace_id, fleet_id, event_id, \
             NULL::uuid AS authoring_attempt_id, charge_type, posture, model, \
             fleet_name, credit_deducted_nanos, token_count_input, token_count_output, \
             wall_ms, created_at FROM billing.usage_ledger WHERE tenant_id=$1::uuid \
             UNION ALL \
             SELECT s.id, a.tenant_id, a.workspace_id, f.id, NULL::text, a.id, \
             'authoring', CASE WHEN (a.model_snapshot->>'platform_managed')::boolean \
             THEN 'platform' ELSE 'self_managed' END, a.model_snapshot->>'model', \
             f.name, s.charged_nanos, (s.usage->>'input_tokens')::bigint, \
             (s.usage->>'output_tokens')::bigint, (s.usage->>'runtime_ms')::bigint, s.created_at \
             FROM billing.authoring_settlements s \
             JOIN fleet.authoring_attempts a ON a.id=s.attempt_id \
             LEFT JOIN core.authoring_drafts d ON d.id=a.draft_id \
               AND d.tenant_id=a.tenant_id AND d.workspace_id=a.workspace_id \
             LEFT JOIN core.fleets f ON f.id=d.linked_fleet_id \
               AND f.tenant_id=a.tenant_id AND f.workspace_id=a.workspace_id \
             WHERE a.tenant_id=$1::uuid) \
             SELECT id::text, tenant_id::text, workspace_id::text, fleet_id::text, \
             event_id, authoring_attempt_id::text, charge_type, posture, model, fleet_name, \
             credit_deducted_nanos, token_count_input, token_count_output, wall_ms, created_at \
             FROM charges ",
            $boundary,
            " ORDER BY charges.created_at DESC, charges.id DESC LIMIT ",
            $limit
        )
    };
}

/// Newest first. `$1` tenant, `$2` limit.
pub(crate) const SELECT_TENANT_CHARGES_PAGE_FIRST: &str = charges_page!("", "$2");

/// Strictly before the prior page. `$1` tenant, `$2` time, `$3` UUID, `$4` limit.
pub(crate) const SELECT_TENANT_CHARGES_PAGE_AFTER: &str =
    charges_page!("WHERE (created_at, id) < ($2, $3::uuid)", "$4");
