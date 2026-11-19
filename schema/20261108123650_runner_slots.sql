-- The claim on one conversation: who may run it, the next fencing token, the sticky runner
-- and the sandbox hold. Kept off the owner's row so claims do not lock the row every edit
-- compares against. A fleet or a setup draft owns each slot, never both, and `id` is that
-- owner's id, so the run statements both kinds share address any slot by `id`. `fleet_id` is
-- read by the fleet claim, renewal and report statements; `draft_id` by the draft claim.
CREATE TABLE runner.slots (
    id                          UUID PRIMARY KEY,
    fleet_id                    UUID REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    draft_id                    UUID REFERENCES authoring.drafts(id) ON DELETE CASCADE,
    last_runner_id              UUID REFERENCES runner.runners(id) ON DELETE SET NULL,
    fencing_token               BIGINT NOT NULL,
    run_expires_at            TIMESTAMPTZ(3) NOT NULL,
    hold_expires_at             TIMESTAMPTZ(3),
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    updated_at                  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_slots_one_owner
        CHECK (num_nonnulls(fleet_id, draft_id) = 1 AND id = COALESCE(fleet_id, draft_id))
);

-- Serve each owner's ON DELETE CASCADE. Neither column changes after insert, so neither
-- index blocks the heap-only update every renewal makes.
CREATE INDEX idx_slots_fleet_id ON runner.slots (fleet_id) WHERE fleet_id IS NOT NULL;
CREATE INDEX idx_slots_draft_id ON runner.slots (draft_id) WHERE draft_id IS NOT NULL;

CREATE INDEX idx_slots_last_runner_id_held
    ON runner.slots (last_runner_id) WHERE hold_expires_at IS NOT NULL;

-- The runner alone: every renewal moves `run_expires_at`, and an index on it would
-- block heap-only updates. Serves the foreign key's SET NULL when a runner goes.
CREATE INDEX idx_slots_last_runner_id ON runner.slots (last_runner_id);

GRANT SELECT, INSERT, UPDATE ON runner.slots TO api_runtime;
