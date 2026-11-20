-- One row per turn, keyed by its parent: replacement runs share this head.
-- head_turn_checkpoint_id is read by assign/restore; next_generation by begin.
CREATE TABLE runner.turn_recovery (
    turn_id UUID PRIMARY KEY,
    head_turn_checkpoint_id UUID,
    next_generation BIGINT NOT NULL CHECK (next_generation > 0),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    -- Recovery state is the turn's own, so it goes with the turn.
    CONSTRAINT fk_turn_recovery_turn_id FOREIGN KEY (turn_id)
        REFERENCES thread.turns (id) ON DELETE CASCADE
);

CREATE INDEX idx_turn_recovery_head ON runner.turn_recovery (head_turn_checkpoint_id, turn_id)
    WHERE head_turn_checkpoint_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.turn_recovery TO api_runtime;
