WITH page AS MATERIALIZED (
    SELECT id, tenant_id, workspace_id, fleet_id, fleet_name, event_id, authoring_attempt_id,
           work_type, posture, model, charged_nanos, input_token_count, output_token_count,
           runtime_ms, created_at
      FROM billing.usage_lines
     WHERE tenant_id = $1::uuid
       AND (created_at, id) < ($2::timestamptz, $3::uuid)
     ORDER BY created_at DESC, id DESC
     LIMIT $4
)
SELECT p.id::text, p.tenant_id::text, p.workspace_id::text,
       COALESCE(p.fleet_id, f.id)::text AS fleet_id, COALESCE(p.fleet_name, f.name) AS fleet_name,
       p.event_id, p.authoring_attempt_id::text,
       CASE WHEN p.work_type = $6 THEN $7 ELSE $5 END AS charge_type,
       p.posture, p.model, p.charged_nanos, p.input_token_count, p.output_token_count,
       p.runtime_ms AS wall_ms, p.created_at
  FROM page p
  LEFT JOIN authoring.attempts a ON a.id = p.authoring_attempt_id AND a.tenant_id = p.tenant_id
  LEFT JOIN authoring.drafts d ON d.id = a.draft_id
        AND d.tenant_id = a.tenant_id AND d.workspace_id = a.workspace_id
  LEFT JOIN fleet.fleets f ON f.id = d.fleet_id
        AND f.tenant_id = a.tenant_id AND f.workspace_id = a.workspace_id
 ORDER BY p.created_at DESC, p.id DESC
