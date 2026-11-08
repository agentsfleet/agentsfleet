INSERT INTO authoring.messages
    (id, draft_id, sequence, operation_id, status, tenant_model_id, user_text, save_requested, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4::uuid, $5, $6::uuid, $7, $8, now(), now())
