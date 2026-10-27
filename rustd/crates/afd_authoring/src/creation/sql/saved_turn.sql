SELECT d.fleet_id::text FROM authoring.drafts d
 JOIN authoring.messages m ON m.draft_id = d.id
 WHERE d.id = $1::uuid AND d.workspace_id = $2::uuid AND d.user_id = $3::uuid
   AND m.sequence = $4 AND m.save_requested AND m.status = $5
   AND d.status = $6 AND d.fleet_id IS NOT NULL
