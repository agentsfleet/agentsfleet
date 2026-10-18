WITH runtime_page AS MATERIALIZED (
    SELECT id, tenant_id, workspace_id, fleet_id, fleet_name, event_id, charge_type,
           posture, model, charged_nanos, input_token_count, output_token_count,
           wall_ms, created_at
      FROM billing.fleet_charges
     WHERE tenant_id = $1::uuid
     ORDER BY created_at DESC, id DESC
     LIMIT $2
), setup_page AS MATERIALIZED (
    SELECT id, tenant_id, attempt_id, usage, charged_nanos, created_at
      FROM billing.authoring_settlements
     WHERE tenant_id = $1::uuid
     ORDER BY created_at DESC, id DESC
     LIMIT $2
), charges AS (
    SELECT id, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
           NULL::uuid AS authoring_attempt_id, charge_type, posture, model,
           charged_nanos, input_token_count, output_token_count, wall_ms, created_at
      FROM runtime_page
    UNION ALL
    SELECT s.id, a.tenant_id, a.workspace_id, f.id, f.name, NULL::text, a.id,
           $3,
           CASE WHEN (a.model_snapshot->>'platform_managed')::boolean
                THEN $4 ELSE $5 END,
           a.model_snapshot->>'model', s.charged_nanos,
           (s.usage->>'input_tokens')::bigint, (s.usage->>'output_tokens')::bigint,
           a.reported_runtime_ms, s.created_at
      FROM setup_page s
      JOIN authoring.attempts a ON a.id = s.attempt_id AND a.tenant_id = s.tenant_id
      LEFT JOIN authoring.drafts d ON d.id = a.draft_id
            AND d.tenant_id = a.tenant_id AND d.workspace_id = a.workspace_id
      LEFT JOIN fleet.fleets f ON f.id = d.fleet_id
            AND f.tenant_id = a.tenant_id AND f.workspace_id = a.workspace_id
)
SELECT id::text, tenant_id::text, workspace_id::text, fleet_id::text, fleet_name,
       event_id, authoring_attempt_id::text, charge_type, posture, model, charged_nanos,
       input_token_count, output_token_count, wall_ms, created_at
  FROM charges
 ORDER BY charges.created_at DESC, charges.id DESC
 LIMIT $2
