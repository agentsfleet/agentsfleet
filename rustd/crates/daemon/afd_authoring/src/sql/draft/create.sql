INSERT INTO authoring.drafts
    (id, workspace_id, tenant_id, user_id, title, status, tenant_model_id,
     source_type, source_library_revision_id, source_fleet_revision_id, instance_name,
     fleet_id, last_message_sequence, cancelled_through_sequence, resource_version,
     created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7::uuid,
        $8, $9::uuid, $10::uuid, $11,
        $12::uuid, $13, $13, $13,
        now(), now())
