SELECT id, page_number, digest, body FROM fleet.durable_task_checkpoint_pages
WHERE id = $1 AND durable_task_checkpoint_id = $2
UNION ALL
SELECT id, page_number, digest, body FROM fleet.durable_task_base_pages
WHERE id = $1 AND durable_task_base_id = $3
