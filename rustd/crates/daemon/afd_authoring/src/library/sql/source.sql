SELECT d.id::text
  FROM authoring.drafts d
  JOIN authoring.candidates c ON c.draft_id = d.id
 WHERE c.id = $1::uuid AND d.workspace_id = $2::uuid AND d.user_id = $3::uuid
