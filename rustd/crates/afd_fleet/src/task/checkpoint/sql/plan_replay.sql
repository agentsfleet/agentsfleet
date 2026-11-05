SELECT checkpoint_id::text FROM fleet.task_checkpoint_operations WHERE task_id = $1::uuid AND operation_id = $2::uuid
