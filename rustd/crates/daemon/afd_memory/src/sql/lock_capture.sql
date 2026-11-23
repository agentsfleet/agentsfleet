-- NO KEY UPDATE: the grants hold still for the write, and a foreign-key check on the fleet passes.
SELECT workspace_id, memory_reads_workspace, memory_publishes_workspace
FROM fleet.fleets WHERE id = $1
FOR NO KEY UPDATE
