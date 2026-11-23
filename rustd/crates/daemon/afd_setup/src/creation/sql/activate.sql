UPDATE fleet.fleets f
   SET status = $3, resource_version = f.resource_version + 1, updated_at = now()
  FROM fleet.revisions r
 WHERE f.id = $1 AND f.workspace_id = $2 AND f.status = $4
   AND r.id = f.head_revision_id AND r.revision = $5
   AND f.setup_stream_ready = true
