SELECT workspace_id, memory_reads_workspace, memory_publishes_workspace
FROM fleet.fleets WHERE id = $1
