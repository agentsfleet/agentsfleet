SELECT usage_unknown
  FROM authoring.attempts
 WHERE draft_id = $1::uuid AND message_sequence > $3
   AND (status <> $2 OR usage_unknown)
 ORDER BY message_sequence DESC
 LIMIT 1
