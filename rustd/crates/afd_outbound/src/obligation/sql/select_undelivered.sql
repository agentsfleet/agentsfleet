SELECT id::text AS id, fleet_id::text AS fleet_id, workspace_id::text AS workspace_id,
       provider, destination, event_id, answer
  FROM fleet.obligations
 WHERE receipt IS NOT NULL AND delivered_at IS NULL AND abandoned_at IS NULL
   AND updated_at < statement_timestamp() - $1::interval
 ORDER BY fleet_id, created_at, sequence
 LIMIT $2::bigint
