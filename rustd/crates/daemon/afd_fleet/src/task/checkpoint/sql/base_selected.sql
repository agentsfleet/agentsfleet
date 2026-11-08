SELECT id, page_ids, manifest_root, logical_bytes, entry_count, ready_at IS NOT NULL AS ready
FROM fleet.task_bases
WHERE id = $1 AND workspace_id = $2 AND repository = $3
  AND format_version = $4 AND toolbox_digest = $5 AND retired_at IS NULL
FOR UPDATE
