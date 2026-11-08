UPDATE fleet.task_object_pins SET released_at = clock_timestamp(), updated_at = now() WHERE id = $1
