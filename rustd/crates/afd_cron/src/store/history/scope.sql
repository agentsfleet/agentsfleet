-- Session foreign-key checks must progress while this transaction waits on a child.
SELECT f.workspace_id::text
  FROM fleet.fleets f
 WHERE f.id = $1::uuid
   FOR NO KEY UPDATE OF f
