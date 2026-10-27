SELECT memory_reads_workspace, memory_publishes_workspace, txid_current()::text AS activity_id
  FROM fleet.fleets
 WHERE id = $1::uuid AND workspace_id = $2::uuid
 FOR UPDATE
