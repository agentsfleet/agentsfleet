UPDATE fleet.tasks SET covered_input_sequence = $2, resource_version = resource_version + 1,
    updated_at = clock_timestamp()
WHERE id = $1 AND covered_input_sequence = $2 - 1 AND input_sequence >= $2
