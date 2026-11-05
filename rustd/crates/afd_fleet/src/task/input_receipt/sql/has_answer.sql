SELECT EXISTS (SELECT 1 FROM fleet.task_input_receipts WHERE task_item_id = $1::uuid AND response_text IS NOT NULL)
