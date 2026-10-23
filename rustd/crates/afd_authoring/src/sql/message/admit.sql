WITH prior AS MATERIALIZED (
    SELECT r.user_id::text AS user_id, r.request_digest,
           r.resource_id::text AS resource_id,
           EXISTS (SELECT 1 FROM authoring.messages m WHERE m.id = r.resource_id) AS live
      FROM authoring.replay_receipts r
     WHERE r.scope = $11 AND r.operation_id = $4::uuid
       AND r.created_at > statement_timestamp() - $13::interval
), pending AS MATERIALIZED (
    SELECT usage_unknown
      FROM authoring.attempts
     WHERE draft_id = $1::uuid AND message_sequence > $14
       AND (status <> $15 OR usage_unknown)
     ORDER BY message_sequence DESC
     LIMIT 1
), eligible AS MATERIALIZED (
    SELECT $2::bigint AS sequence
     WHERE $2::bigint IS NOT NULL
       AND $3::uuid IS NOT NULL AND $8::uuid IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM prior)
       AND NOT COALESCE((SELECT usage_unknown FROM pending), false)
), expired AS (
    DELETE FROM authoring.replay_receipts
     WHERE scope = $11 AND operation_id = $4::uuid
       AND created_at <= statement_timestamp() - $13::interval
       AND EXISTS (SELECT 1 FROM eligible)
    RETURNING id
), message AS (
    INSERT INTO authoring.messages
        (id, draft_id, sequence, operation_id, status, tenant_model_id,
         user_text, created_at, updated_at)
    SELECT $3::uuid, $1::uuid, e.sequence, $4::uuid, $5, $6::uuid,
           $7, now(), now()
      -- Finish deleting an expired receipt before its unique key is reused.
      FROM eligible e CROSS JOIN (SELECT count(*) FROM expired) expired_done
    RETURNING id
), receipt AS (
    INSERT INTO authoring.replay_receipts
        (id, workspace_id, user_id, scope, operation_id, request_digest,
         resource_id, created_at)
    SELECT $8::uuid, $9::uuid, $10::uuid, $11, $4::uuid, $12, m.id, now()
      FROM message m
    ON CONFLICT (scope, operation_id) DO NOTHING
    RETURNING id
), advanced AS (
    UPDATE authoring.drafts
       SET last_message_sequence = $2,
           resource_version = resource_version + 1,
           updated_at = now()
     WHERE id = $1::uuid AND EXISTS (SELECT 1 FROM receipt)
    RETURNING last_message_sequence
)
SELECT (SELECT row_to_json(prior) FROM prior) AS prior,
       (SELECT m.sequence FROM authoring.messages m JOIN prior p ON m.id = p.resource_id::uuid
         WHERE m.draft_id = $1::uuid) AS prior_sequence,
       COALESCE((SELECT usage_unknown FROM pending), false) AS usage_unknown,
       (SELECT last_message_sequence FROM advanced) AS sequence
