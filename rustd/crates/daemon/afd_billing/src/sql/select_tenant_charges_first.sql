WITH page AS MATERIALIZED (
    SELECT id, tenant_id, workspace_id, fleet_id, fleet_name, event_id, authoring_attempt_id,
           work_type, posture, model, charged_nanos, input_token_count, output_token_count,
           runtime_ms, created_at
      FROM billing.usage_lines
     WHERE tenant_id = $1
     ORDER BY created_at DESC, id DESC
     LIMIT $2
)
SELECT p.id, p.tenant_id, p.workspace_id,
       COALESCE(p.fleet_id, f.id) AS fleet_id, COALESCE(p.fleet_name, f.name) AS fleet_name,
       -- A setup line names the draft message it ran in event_id; the charge shows no event.
       CASE WHEN p.work_type = $4 THEN p.event_id END AS event_id, p.authoring_attempt_id,
       CASE WHEN p.work_type = $4 THEN $5 ELSE $3 END AS charge_type,
       p.posture, p.model, p.charged_nanos, p.input_token_count, p.output_token_count,
       p.runtime_ms AS wall_ms, p.created_at
  FROM page p
  -- Setup turns the authoring path still runs; this join goes with authoring.attempts.
  LEFT JOIN authoring.attempts a ON a.id = p.authoring_attempt_id AND a.tenant_id = p.tenant_id
  -- A draft lease's lines name the lease as their authoring_attempt_id.
  LEFT JOIN runner.leases dl ON dl.id = p.authoring_attempt_id AND dl.draft_id IS NOT NULL
        AND dl.tenant_id = p.tenant_id
  LEFT JOIN authoring.drafts d ON d.id = COALESCE(a.draft_id, dl.draft_id)
        AND d.tenant_id = p.tenant_id AND d.workspace_id = p.workspace_id
  LEFT JOIN fleet.fleets f ON f.id = d.fleet_id
        AND f.tenant_id = p.tenant_id AND f.workspace_id = p.workspace_id
 ORDER BY p.created_at DESC, p.id DESC
