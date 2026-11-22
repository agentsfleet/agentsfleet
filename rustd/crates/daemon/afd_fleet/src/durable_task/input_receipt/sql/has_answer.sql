SELECT EXISTS (SELECT 1 FROM fleet.durable_task_input_receipts WHERE durable_task_item_id = $1 AND response_text IS NOT NULL)
