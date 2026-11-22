SELECT d.id
  FROM setup.drafts d
  JOIN fleet.revisions r ON r.draft_id = d.id OR r.id = d.head_revision_id
 WHERE r.id = $1 AND d.workspace_id = $2 AND d.user_id = $3
