SELECT memory_reads_workspace, memory_publishes_workspace
  FROM fleet.fleets
 WHERE id = $1 AND workspace_id = $2
 FOR UPDATE
