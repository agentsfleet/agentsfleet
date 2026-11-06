UPDATE fleet.tasks SET input_sequence = input_sequence + 1,
    generation = generation + $2::boolean::integer,
    stop_generation = stop_generation + $2::boolean::integer,
    status = CASE WHEN $2 THEN $3 ELSE status END,
    resource_version = resource_version + 1, updated_at = clock_timestamp()
WHERE id = $1::uuid RETURNING input_sequence, generation
