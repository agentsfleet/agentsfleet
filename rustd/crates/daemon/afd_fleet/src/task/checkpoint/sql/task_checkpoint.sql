UPDATE fleet.tasks SET task_checkpoint_id = $2::uuid, checkpoint_cleanup_at = clock_timestamp(),
    resource_version = resource_version + 1, updated_at = now() WHERE id = $1::uuid
