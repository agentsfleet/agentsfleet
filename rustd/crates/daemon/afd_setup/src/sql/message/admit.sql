WITH prior AS MATERIALIZED (
    SELECT r.request_digest, r.resource_id,
           EXISTS (SELECT 1 FROM thread.turns m WHERE m.id = r.resource_id) AS live
      FROM thread.replay_receipts r
     WHERE r.scope = $10 AND r.operation_id = $4
       AND r.created_at > statement_timestamp() - $12::interval
), eligible AS MATERIALIZED (
    SELECT $2::bigint AS sequence
     WHERE $2::bigint IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM prior)
), expired AS (
    DELETE FROM thread.replay_receipts
     WHERE scope = $10 AND operation_id = $4
       AND created_at <= statement_timestamp() - $12::interval
       AND EXISTS (SELECT 1 FROM eligible)
    RETURNING operation_id
), message AS (
    -- A setup turn draws its sequence from the draft, names its sender by subject and
    -- carries its text as the body a steer carries.
    INSERT INTO thread.turns
        (id, thread_id, workspace_id, sequence, operation_id, kind, actor, status,
         tenant_model_id, request_json, save_requested, created_at, updated_at)
    SELECT $3, $1, $8, e.sequence, $4, $14,
           (SELECT u.oidc_subject FROM identity.users u WHERE u.id = $9), $5, $6,
           jsonb_build_object('message', $7::text), $13, now(), now()
      -- Finish deleting an expired receipt before its unique key is reused.
      FROM eligible e CROSS JOIN (SELECT count(*) FROM expired) expired_done
    RETURNING id
), receipt AS (
    INSERT INTO thread.replay_receipts
        (scope, operation_id, request_digest, resource_id, created_at)
    SELECT $10, $4, $11, m.id, now()
      FROM message m
    ON CONFLICT (scope, operation_id) DO NOTHING
    RETURNING operation_id
), advanced AS (
    UPDATE setup.drafts
       SET resource_version = resource_version + 1,
           updated_at = now()
     WHERE id = $1 AND EXISTS (SELECT 1 FROM receipt)
    RETURNING $2::bigint AS sequence
)
SELECT (SELECT row_to_json(prior) FROM prior) AS prior,
       (SELECT m.sequence FROM thread.turns m JOIN prior p ON m.id = p.resource_id
         WHERE m.thread_id = $1) AS prior_sequence,
       (SELECT sequence FROM advanced) AS sequence
