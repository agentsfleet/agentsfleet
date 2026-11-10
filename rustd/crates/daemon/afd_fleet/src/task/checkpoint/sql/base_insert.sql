INSERT INTO fleet.task_bases (id, workspace_id, repository, commit_oid, format_version, toolbox_digest,
    page_count, page_ids, logical_bytes, entry_count, retained_bytes, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0, now(), now())
