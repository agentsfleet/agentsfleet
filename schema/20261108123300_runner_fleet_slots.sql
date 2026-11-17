-- The per-fleet claim: who may lease it, the next fencing token, the sticky runner and the
-- sandbox hold. Kept off the fleet row so claims do not lock the row every edit compares against.
CREATE TABLE runner.fleet_slots (
    fleet_id                    UUID PRIMARY KEY REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    last_runner_id              UUID REFERENCES runner.runners(id) ON DELETE SET NULL,
    fencing_token               BIGINT NOT NULL,
    lease_expires_at            TIMESTAMPTZ(3) NOT NULL,
    hold_expires_at             TIMESTAMPTZ(3),
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    updated_at                  TIMESTAMPTZ(3) NOT NULL
);

CREATE INDEX idx_fleet_slots_last_runner_id_held
    ON runner.fleet_slots (last_runner_id) WHERE hold_expires_at IS NOT NULL;

-- The runner alone: every renewal moves `lease_expires_at`, and an index on it would
-- block heap-only updates. Serves the foreign key's SET NULL when a runner goes.
CREATE INDEX idx_fleet_slots_last_runner_id ON runner.fleet_slots (last_runner_id);

GRANT SELECT, INSERT, UPDATE ON runner.fleet_slots TO api_runtime;
