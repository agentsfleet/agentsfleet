SELECT d.id::text AS id,
       d.workspace_id::text AS workspace_id,
       d.tenant_id::text AS tenant_id,
       d.user_id::text AS user_id
  FROM authoring.drafts d
  JOIN authoring.messages m ON m.draft_id = d.id
 WHERE d.status = $1 AND m.status = $2 AND m.sequence > d.cancelled_through_sequence
   AND NOT EXISTS (SELECT 1 FROM authoring.attempts a
                    WHERE a.draft_id = d.id
                      AND a.message_sequence > d.cancelled_through_sequence
                      AND (a.status <> $3 OR a.usage_unknown))
 GROUP BY d.id
 ORDER BY min(m.created_at), d.id
 LIMIT $4
