-- Durable side-effect outbox + dead-letter baseline
CREATE TABLE core.run_side_effect_outbox (
    id               UUID PRIMARY KEY,
    CONSTRAINT ck_run_side_effect_outbox_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id           UUID   NOT NULL REFERENCES core.runs(run_id),
    effect_key       TEXT   NOT NULL,
    status           TEXT   NOT NULL,
    last_event       TEXT   NOT NULL,
    payload          TEXT,
    reconciled_state TEXT,
    created_at       BIGINT NOT NULL,
    updated_at       BIGINT NOT NULL,
    UNIQUE (run_id, effect_key)
);

CREATE INDEX idx_side_effect_outbox_status_updated
    ON core.run_side_effect_outbox(status, updated_at, run_id);

INSERT INTO core.run_side_effect_outbox (
    run_id,
    effect_key,
    status,
    last_event,
    payload,
    reconciled_state,
    created_at,
    updated_at
)
SELECT
    run_id,
    effect_key,
    CASE status
        WHEN 'done' THEN 'delivered'
        WHEN 'dead_letter' THEN 'dead_letter'
        ELSE 'pending'
    END AS status,
    CASE status
        WHEN 'done' THEN 'done'
        WHEN 'dead_letter' THEN 'reconciled_dead_letter'
        ELSE 'claimed'
    END AS last_event,
    details,
    NULL,
    created_at,
    updated_at
FROM core.run_side_effects
ON CONFLICT (run_id, effect_key) DO NOTHING;

GRANT SELECT, INSERT, UPDATE, DELETE ON core.run_side_effect_outbox TO api_runtime;
GRANT SELECT, INSERT, UPDATE ON core.run_side_effect_outbox TO worker_runtime;
