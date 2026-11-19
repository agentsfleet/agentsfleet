-- A setup turn draws its sequence from the draft, names its sender by subject and carries
-- its text as the body a steer carries.
INSERT INTO thread.turns
    (id, thread_id, workspace_id, sequence, operation_id, kind, actor, status,
     tenant_model_id, request_json, save_requested, created_at, updated_at)
VALUES ($1, $2, $9, $3, $4, $11,
        (SELECT u.oidc_subject FROM identity.users u WHERE u.id = $10), $5, $6,
        jsonb_build_object('message', $7::text), $8, now(), now())
