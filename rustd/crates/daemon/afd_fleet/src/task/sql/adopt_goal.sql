UPDATE fleet.tasks
SET accepted_revision_id = $2::uuid, questions = $3, goal_digest = $4,
    resource_version = resource_version + 1, generation = generation + 1,
    stop_generation = stop_generation + 1,
    status = CASE WHEN $6 THEN $5 ELSE status END,
    checkpoint_cleanup_at = CASE WHEN checkpoint_sequence IS NOT NULL THEN clock_timestamp() ELSE checkpoint_cleanup_at END,
    updated_at = now()
WHERE id = $1::uuid
