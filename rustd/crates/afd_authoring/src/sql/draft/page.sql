-- The summaries a chat list shows without opening each chat: the current
-- candidate's revision, the saved fleet's current setup revision, and the
-- published Library revision. Each is one primary-key probe per row; a missing
-- row (no candidate yet, no saved fleet, never published) reads null.
SELECT d.id::text AS id,
       d.title,
       d.status,
       d.resource_version,
       d.fleet_id::text AS fleet_id,
       d.updated_at,
       d.expires_at,
       c.revision AS candidate_revision,
       fr.revision AS setup_revision,
       lr.revision AS publication_revision
  FROM authoring.drafts d
  LEFT JOIN authoring.candidates c ON c.id = d.head_candidate_id
  LEFT JOIN fleet.fleets f ON f.id = d.fleet_id
  LEFT JOIN fleet.revisions fr ON fr.id = f.head_revision_id
  LEFT JOIN authoring.library_revisions lr ON lr.id = d.library_revision_id
 WHERE d.workspace_id = $1::uuid AND d.user_id = $2::uuid
   AND (d.expires_at IS NULL OR d.expires_at > statement_timestamp())
   AND ($3::text IS NULL OR d.status = $3)
   AND ($4::uuid IS NULL OR d.fleet_id = $4::uuid)
   AND ($5::timestamptz IS NULL OR (d.updated_at, d.id) < ($5::timestamptz, $6::uuid))
 ORDER BY d.updated_at DESC, d.id DESC
 LIMIT $7
