SELECT input_sequence, target->>'head' AS head FROM fleet.tasks WHERE id = $1::uuid
