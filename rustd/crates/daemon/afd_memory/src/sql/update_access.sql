UPDATE fleet.fleets
SET memory_reads_workspace = COALESCE($3, memory_reads_workspace),
    memory_publishes_workspace = COALESCE($4, memory_publishes_workspace),
    updated_at = now()
WHERE id = $1 AND workspace_id = $2
RETURNING memory_reads_workspace, memory_publishes_workspace
