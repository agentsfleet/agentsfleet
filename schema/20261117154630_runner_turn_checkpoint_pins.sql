-- A reader pins an exact generation; renewal never extends its operation deadline.
CREATE TABLE runner.turn_checkpoint_pins (
    id UUID PRIMARY KEY,
    turn_checkpoint_id UUID NOT NULL REFERENCES runner.turn_checkpoints (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    operation_id UUID NOT NULL,
    owner_lease_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL CHECK (owner_fence > 0),
    resource_version BIGINT NOT NULL CHECK (resource_version > 0),
    expires_at TIMESTAMPTZ(3) NOT NULL,
    operation_expires_at TIMESTAMPTZ(3) NOT NULL,
    released_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_turn_pin_deadline CHECK (expires_at <= operation_expires_at)
);
CREATE UNIQUE INDEX uq_turn_pin_live ON runner.turn_checkpoint_pins (turn_checkpoint_id, owner_lease_id, kind)
    WHERE released_at IS NULL;
CREATE UNIQUE INDEX uq_turn_pin_operation ON runner.turn_checkpoint_pins
    (turn_checkpoint_id, owner_lease_id, kind, operation_id);
CREATE INDEX idx_turn_pins_expiry ON runner.turn_checkpoint_pins (expires_at, id)
    WHERE released_at IS NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON runner.turn_checkpoint_pins TO api_runtime;
