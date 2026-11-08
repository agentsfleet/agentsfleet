INSERT INTO authoring.messages
    (id, draft_id, sequence, operation_id, status, tenant_model_id, user_text, save_requested, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now())
