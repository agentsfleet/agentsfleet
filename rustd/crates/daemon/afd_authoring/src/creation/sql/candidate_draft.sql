SELECT d.id
  FROM authoring.drafts d
  JOIN authoring.candidates c ON c.draft_id = d.id
 WHERE c.id = $1 AND d.workspace_id = $2 AND d.user_id = $3
