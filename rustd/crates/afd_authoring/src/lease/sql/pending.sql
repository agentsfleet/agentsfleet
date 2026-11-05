SELECT usage_unknown
  FROM authoring.attempts
 WHERE draft_id = $1::uuid AND message_sequence > $3
   AND status <> ALL($2::text[])
 ORDER BY message_sequence DESC
 LIMIT 1
