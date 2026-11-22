SELECT id, page_number, digest, body FROM fleet.durable_task_checkpoint_pages
WHERE durable_task_checkpoint_id = $1 AND NOT $3
UNION ALL
SELECT id, page_number, digest, body FROM fleet.durable_task_base_pages WHERE durable_task_base_id = $2 AND $3
ORDER BY page_number
