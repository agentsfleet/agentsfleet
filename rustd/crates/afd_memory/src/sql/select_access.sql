SELECT workspace_id::text AS workspace_id, memory_reads_workspace, memory_publishes_workspace
FROM fleet.fleets WHERE id = $1::uuid
