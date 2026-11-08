WITH locked AS MATERIALIZED (
    SELECT id, workspace_id, tenant_id, user_id, title, status, tenant_model_id,
           head_candidate_id, source_type, source_library_revision_id,
           source_fleet_revision_id, instance_name, fleet_id, library_revision_id,
           last_message_sequence, cancelled_through_sequence, resource_version,
           closed_at, expires_at
      FROM authoring.drafts
     WHERE id = $1::uuid AND workspace_id = $2::uuid AND user_id = $3::uuid
       FOR UPDATE
), clock AS MATERIALIZED (
    SELECT clock_timestamp() AS locked_at FROM locked
)
SELECT d.id::text AS id,
       d.status,
       d.title,
       d.resource_version,
       d.tenant_model_id::text AS tenant_model_id,
       d.head_candidate_id::text AS head_candidate_id,
       COALESCE(c.revision, 0) AS candidate_revision,
       d.last_message_sequence,
       d.cancelled_through_sequence,
       d.fleet_id::text AS fleet_id,
       d.closed_at,
       d.expires_at,
       d.workspace_id::text AS workspace_id,
       d.tenant_id::text AS tenant_id,
       d.user_id::text AS user_id,
       d.instance_name,
       d.source_type,
       COALESCE(sl.platform_slug, sl.id::text) AS source_library_id,
       sr.id::text AS source_library_revision_id,
       sf.fleet_id::text AS source_fleet_id,
       sf.revision AS source_fleet_revision,
       COALESCE(pl.platform_slug, pl.id::text) AS publication_library_id,
       pr.id::text AS publication_revision_id,
       pr.canonical_digest AS publication_digest
  FROM locked d
 CROSS JOIN clock
  LEFT JOIN authoring.candidates c ON c.id = d.head_candidate_id
  LEFT JOIN authoring.library_revisions sr ON sr.id = d.source_library_revision_id
  LEFT JOIN authoring.library_entries sl ON sl.id = sr.library_entry_id
  LEFT JOIN fleet.revisions sf ON sf.id = d.source_fleet_revision_id
  LEFT JOIN authoring.library_revisions pr ON pr.id = d.library_revision_id
  LEFT JOIN authoring.library_entries pl ON pl.id = pr.library_entry_id
 WHERE d.expires_at IS NULL OR d.expires_at > clock.locked_at
