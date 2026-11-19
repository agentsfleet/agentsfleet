SELECT r.user_id AS user_id, r.request_digest, r.resource_id AS resource_id,
       CASE $4
           WHEN 'fleet' THEN EXISTS (SELECT 1 FROM fleet.fleets f WHERE f.id = r.resource_id)
           WHEN 'fleet_revision' THEN
               EXISTS (SELECT 1 FROM fleet.revisions v WHERE v.id = r.resource_id)
           WHEN 'draft' THEN EXISTS (SELECT 1 FROM setup.drafts d WHERE d.id = r.resource_id)
           WHEN 'message' THEN
               EXISTS (SELECT 1 FROM setup.messages m WHERE m.id = r.resource_id)
           WHEN 'plugin_revision' THEN
               EXISTS (SELECT 1 FROM setup.plugin_revisions l WHERE l.id = r.resource_id)
       END AS live
  FROM setup.replay_receipts r
 WHERE r.scope = $1 AND r.operation_id = $2
   AND r.created_at > statement_timestamp() - $3::interval
