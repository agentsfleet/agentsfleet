SELECT 1::bigint
  FROM fleet.fleets
 WHERE id = $1::uuid AND workspace_id = $2::uuid
 LIMIT 1
