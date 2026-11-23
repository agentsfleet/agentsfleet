SELECT input_sequence, target->>'head' AS head FROM fleet.durable_tasks WHERE id = $1
