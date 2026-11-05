SELECT id::text, page_number, digest, body FROM fleet.task_checkpoint_pages
WHERE id = $1::uuid AND checkpoint_id = $2::uuid
UNION ALL
SELECT id::text, page_number, digest, body FROM fleet.task_base_pages
WHERE id = $1::uuid AND base_id = $3::uuid
