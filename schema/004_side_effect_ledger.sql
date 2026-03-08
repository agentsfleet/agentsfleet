-- Side-effect idempotency ledger (M4_004 D14)
CREATE TABLE IF NOT EXISTS run_side_effects (
    id         BIGSERIAL PRIMARY KEY,
    run_id     TEXT   NOT NULL REFERENCES runs(run_id),
    effect_key TEXT   NOT NULL,
    status     TEXT   NOT NULL DEFAULT 'claimed', -- claimed|done|dead_letter
    details    TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE (run_id, effect_key)
);

CREATE INDEX IF NOT EXISTS idx_side_effects_run_status
    ON run_side_effects(run_id, status, updated_at);
