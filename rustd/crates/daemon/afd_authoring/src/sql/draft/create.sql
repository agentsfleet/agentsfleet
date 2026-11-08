INSERT INTO authoring.drafts
    (id, workspace_id, tenant_id, user_id, title, status, tenant_model_id,
     source_type, source_library_revision_id, source_fleet_revision_id, instance_name,
     fleet_id, last_message_sequence, cancelled_through_sequence, resource_version,
     created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7,
        $8, $9, $10, $11,
        $12, $13, $13, $13,
        now(), now())
