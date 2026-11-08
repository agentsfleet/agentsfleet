UPDATE fleet.tasks SET used_publications = used_publications + 1,
    resource_version = resource_version + 1, updated_at = now() WHERE id = $1
