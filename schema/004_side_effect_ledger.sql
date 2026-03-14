-- Side-effect idempotency ledger
CREATE TABLE run_side_effects (
    id         UUID PRIMARY KEY,
    CONSTRAINT ck_run_side_effects_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id     UUID   NOT NULL REFERENCES runs(run_id),
    effect_key TEXT   NOT NULL,
    status     TEXT   NOT NULL DEFAULT 'claimed',
    details    TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE (run_id, effect_key)
);

CREATE INDEX idx_side_effects_run_status
    ON run_side_effects(run_id, status, updated_at);
