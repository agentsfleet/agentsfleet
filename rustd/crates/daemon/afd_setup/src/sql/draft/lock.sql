WITH locked AS MATERIALIZED (
    SELECT id, workspace_id, tenant_id, user_id, title, status, tenant_model_id,
           head_candidate_id, source_type,
           source_fleet_revision_id, instance_name, fleet_id,
           last_message_sequence, resource_version,
           closed_at, expires_at
      FROM setup.drafts
     WHERE id = $1 AND workspace_id = $2 AND user_id = $3
       FOR UPDATE
), clock AS MATERIALIZED (
    SELECT clock_timestamp() AS locked_at FROM locked
)
SELECT d.id AS id,
       d.status,
       d.title,
       d.resource_version,
       d.tenant_model_id AS tenant_model_id,
       d.head_candidate_id AS head_candidate_id,
       COALESCE(c.revision, 0) AS candidate_revision,
       d.last_message_sequence,
       d.fleet_id AS fleet_id,
       d.closed_at,
       d.expires_at,
       d.workspace_id AS workspace_id,
       d.tenant_id AS tenant_id,
       d.user_id AS user_id,
       d.instance_name,
       d.source_type,
       sf.fleet_id AS source_fleet_id,
       sf.revision AS source_fleet_revision
  FROM locked d
 CROSS JOIN clock
  LEFT JOIN setup.candidates c ON c.id = d.head_candidate_id
  LEFT JOIN fleet.revisions sf ON sf.id = d.source_fleet_revision_id
 WHERE d.expires_at IS NULL OR d.expires_at > clock.locked_at
