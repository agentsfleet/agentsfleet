-- One conversation: who may run it, the next fencing token, the sticky runner, the sandbox
-- hold and the context its next run resumes from. Kept off the owner's row so assignment does
-- not lock the row every edit compares against. A fleet or a setup draft owns each thread, never
-- both, and `id` is that owner's id, so the run statements both kinds share address a thread by
-- `id`. Design and foreign-key reasons: docs/architecture/thread_model.md.
CREATE TABLE thread.threads (
    id                          UUID PRIMARY KEY,
    fleet_id                    UUID REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    draft_id                    UUID REFERENCES setup.drafts(id) ON DELETE CASCADE,
    last_runner_id              UUID REFERENCES runner.runners(id) ON DELETE SET NULL,
    fencing_token               BIGINT NOT NULL,
    run_expires_at            TIMESTAMPTZ(3) NOT NULL,
    hold_expires_at             TIMESTAMPTZ(3),
    resume_context              JSONB,
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    updated_at                  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_threads_one_owner
        CHECK (num_nonnulls(fleet_id, draft_id) = 1 AND id = COALESCE(fleet_id, draft_id))
);

-- Serve each owner's ON DELETE CASCADE. Neither column changes after insert, so neither
-- index blocks the heap-only update every renewal makes.
CREATE INDEX idx_threads_fleet_id ON thread.threads (fleet_id) WHERE fleet_id IS NOT NULL;
CREATE INDEX idx_threads_draft_id ON thread.threads (draft_id) WHERE draft_id IS NOT NULL;

CREATE INDEX idx_threads_last_runner_id_held
    ON thread.threads (last_runner_id) WHERE hold_expires_at IS NOT NULL;

-- The runner alone: every renewal moves `run_expires_at`, and an index on it would
-- block heap-only updates. Serves the foreign key's SET NULL when a runner goes.
CREATE INDEX idx_threads_last_runner_id ON thread.threads (last_runner_id);

GRANT SELECT, INSERT, UPDATE ON thread.threads TO api_runtime;
