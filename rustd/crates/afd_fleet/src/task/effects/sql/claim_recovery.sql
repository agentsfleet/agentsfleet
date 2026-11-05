WITH candidates AS MATERIALIZED (
    SELECT r.effect_id FROM fleet.task_effect_recovery r
    WHERE r.due_at <= statement_timestamp()
    ORDER BY r.due_at, r.effect_id LIMIT $1
    FOR UPDATE OF r SKIP LOCKED
), claimed AS (
    UPDATE fleet.task_effect_recovery r
    SET due_at = clock_timestamp() + make_interval(secs => $2), updated_at = now()
    FROM candidates c WHERE r.effect_id = c.effect_id RETURNING r.*
)
SELECT e.id::text, e.task_id::text, e.fleet_id::text, e.workspace_id::text,
       e.task_generation, e.stop_generation, e.ordinal, e.expected_head, e.tested_tree,
       e.request_digest, NULL::jsonb AS target, s.status AS state, s.commit_oid,
       c.dek_ciphertext, c.dek_nonce, c.dek_tag, c.secret_nonce, c.secret_ciphertext,
       c.secret_tag, c.kek_version, c.credential_expires_at AS expires_at
FROM claimed c JOIN fleet.task_effects e ON e.id = c.effect_id
JOIN fleet.task_effect_states s ON s.effect_id = e.id
