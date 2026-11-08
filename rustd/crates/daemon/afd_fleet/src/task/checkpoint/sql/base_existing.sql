SELECT id::text, page_ids::text[], manifest_root, logical_bytes, entry_count, ready_at IS NOT NULL AS ready
FROM fleet.task_bases
WHERE workspace_id = $1::uuid AND repository = $2 AND commit_oid = $3
  AND format_version = $4 AND toolbox_digest = $5 AND retired_at IS NULL
FOR UPDATE
