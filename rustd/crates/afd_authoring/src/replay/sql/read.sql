SELECT r.user_id::text AS user_id, r.request_digest, r.resource_id::text AS resource_id,
       CASE $4
           WHEN 'fleet' THEN EXISTS (SELECT 1 FROM fleet.fleets f WHERE f.id = r.resource_id)
           WHEN 'fleet_revision' THEN
               EXISTS (SELECT 1 FROM fleet.revisions v WHERE v.id = r.resource_id)
           WHEN 'draft' THEN EXISTS (SELECT 1 FROM authoring.drafts d WHERE d.id = r.resource_id)
           WHEN 'message' THEN
               EXISTS (SELECT 1 FROM authoring.messages m WHERE m.id = r.resource_id)
           WHEN 'library_revision' THEN
               EXISTS (SELECT 1 FROM authoring.library_revisions l WHERE l.id = r.resource_id)
       END AS live
  FROM authoring.replay_receipts r
 WHERE r.scope = $1 AND r.operation_id = $2::uuid
   AND r.created_at > statement_timestamp() - $3::interval
