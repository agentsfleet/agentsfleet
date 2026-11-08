UPDATE fleet.tasks SET effect_cursor = effect_cursor + 1, resource_version = resource_version + 1,
    updated_at = now() WHERE id = $1 RETURNING effect_cursor
