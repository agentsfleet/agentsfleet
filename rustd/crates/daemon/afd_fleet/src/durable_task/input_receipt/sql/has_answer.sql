SELECT EXISTS (SELECT 1 FROM fleet.durable_task_items WHERE id = $1 AND response_text IS NOT NULL)
