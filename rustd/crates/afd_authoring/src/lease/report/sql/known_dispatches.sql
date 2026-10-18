SELECT sequence,
       (usage->>'input_tokens')::bigint AS input_tokens,
       (usage->>'output_tokens')::bigint AS output_tokens,
       created_at
  FROM authoring.dispatches
 WHERE attempt_id = $1::uuid AND usage->>'knowledge' = $2
 ORDER BY sequence
