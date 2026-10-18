SELECT id::text AS id, fleet_id::text AS fleet_id, workspace_id::text AS workspace_id,
       provider, destination, event_id, answer
  FROM fleet.obligations
 WHERE receipt IS NULL AND abandoned_at IS NULL
   AND created_at < statement_timestamp() - $1::interval
 ORDER BY created_at, sequence
 LIMIT $2::bigint
