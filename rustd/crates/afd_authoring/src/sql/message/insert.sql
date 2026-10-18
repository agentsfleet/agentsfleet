INSERT INTO authoring.messages
    (id, draft_id, sequence, status, tenant_model_id, user_text, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5::uuid, $6, now(), now())
