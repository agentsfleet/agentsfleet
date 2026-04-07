-- Side-effect idempotency ledger
CREATE TABLE IF NOT EXISTS core.run_side_effects (
    id         UUID PRIMARY KEY,
    CONSTRAINT ck_run_side_effects_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id     UUID   NOT NULL REFERENCES core.runs(run_id),
    effect_key TEXT   NOT NULL,
    status     TEXT   NOT NULL,
    details    TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE (run_id, effect_key)
);

CREATE INDEX IF NOT EXISTS idx_side_effects_run_status
    ON core.run_side_effects(run_id, status, updated_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.run_side_effects TO api_runtime;
GRANT SELECT, INSERT, UPDATE ON core.run_side_effects TO worker_runtime;
