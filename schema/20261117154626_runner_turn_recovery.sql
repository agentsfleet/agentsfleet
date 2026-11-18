-- One row per event, keyed by its parent: replacement leases share this head.
-- head_turn_checkpoint_id is read by offer/restore; next_generation by begin.
CREATE TABLE runner.turn_recovery (
    event_id UUID PRIMARY KEY REFERENCES fleet.events (event_id) ON DELETE CASCADE,
    head_turn_checkpoint_id UUID,
    next_generation BIGINT NOT NULL CHECK (next_generation > 0),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.turn_recovery TO api_runtime;

