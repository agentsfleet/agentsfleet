SELECT durable_task_checkpoint_id FROM fleet.durable_task_checkpoint_operations WHERE durable_task_id = $1 AND operation_id = $2
