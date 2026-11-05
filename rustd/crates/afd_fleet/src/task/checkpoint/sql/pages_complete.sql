SELECT id::text, page_number, digest, body FROM fleet.task_checkpoint_pages
WHERE checkpoint_id = $1::uuid AND NOT $3
UNION ALL
SELECT id::text, page_number, digest, body FROM fleet.task_base_pages WHERE base_id = $2::uuid AND $3
ORDER BY page_number
