-- The per-fleet claim: who may lease the fleet, the next fencing token, the sticky
-- runner, and the sandbox hold. Keyed by fleet (an identity exception: one per
-- fleet). A table, not fleet columns, because claim traffic would otherwise lock
-- the fleet row every edit compares against.
--
-- `fencing_token` is the last issued lease epoch. `lease_expires_at` and
-- `hold_expires_at` are deadlines; the hold index reads `hold_expires_at`.
-- `metered_*` and `last_metered_at` are the accounting cursor billing reads (rule
-- 4d). Losing the sticky runner clears only the pointer.

CREATE TABLE runner.fleet_slots (
    fleet_id                    UUID PRIMARY KEY REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    last_runner_id              UUID REFERENCES runner.runners(id) ON DELETE SET NULL,
    fencing_token               BIGINT NOT NULL,
    lease_expires_at            TIMESTAMPTZ(3) NOT NULL,
    hold_expires_at             TIMESTAMPTZ(3),
    metered_input_token_count   BIGINT NOT NULL,
    metered_cached_token_count  BIGINT NOT NULL,
    metered_output_token_count  BIGINT NOT NULL,
    last_metered_at             TIMESTAMPTZ(3) NOT NULL,
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    updated_at                  TIMESTAMPTZ(3) NOT NULL
);

-- A runner's held and leased fleets.
CREATE INDEX idx_fleet_slots_last_runner_id_held
    ON runner.fleet_slots (last_runner_id) WHERE hold_expires_at IS NOT NULL;
CREATE INDEX idx_fleet_slots_last_runner_id_lease_expires_at
    ON runner.fleet_slots (last_runner_id, lease_expires_at);

GRANT SELECT, INSERT, UPDATE ON runner.fleet_slots TO api_runtime;
