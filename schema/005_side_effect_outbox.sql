-- Durable side-effect outbox + dead-letter baseline (M4_004 D5/2.1.2)
CREATE TABLE IF NOT EXISTS run_side_effect_outbox (
    id               BIGSERIAL PRIMARY KEY,
    run_id           TEXT   NOT NULL REFERENCES runs(run_id),
    effect_key       TEXT   NOT NULL,
    status           TEXT   NOT NULL DEFAULT 'pending', -- pending|delivered|dead_letter
    last_event       TEXT   NOT NULL, -- claimed|reclaimed|done|reconciled_dead_letter
    payload          TEXT,
    reconciled_state TEXT,
    created_at       BIGINT NOT NULL,
    updated_at       BIGINT NOT NULL,
    UNIQUE (run_id, effect_key)
);

CREATE INDEX IF NOT EXISTS idx_side_effect_outbox_status_updated
    ON run_side_effect_outbox(status, updated_at, run_id);

-- Backfill outbox status from existing side-effect ledger rows.
INSERT INTO run_side_effect_outbox (
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
FROM run_side_effects
ON CONFLICT (run_id, effect_key) DO NOTHING;
