SELECT 1::bigint
  FROM fleet.fleets
 WHERE id = $1 AND workspace_id = $2
 LIMIT 1
