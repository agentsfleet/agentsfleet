-- One conversation: who may run it, the next fencing token, the sticky runner, the sandbox
-- park and the context its next run resumes from. Kept off the fleet's row so assignment does
-- not lock the row every edit compares against. Each fleet owns one thread, and `id` is the
-- fleet's id, so the run statements address a thread by `id`. Design and foreign-key reasons:
-- docs/architecture/thread_model.md.
CREATE TABLE thread.threads (
    id                          UUID PRIMARY KEY,
    fleet_id                    UUID NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    last_runner_id              UUID REFERENCES runner.runners(id) ON DELETE SET NULL,
    fencing_token               BIGINT NOT NULL,
    run_expires_at            TIMESTAMPTZ(3) NOT NULL,
    park_expires_at             TIMESTAMPTZ(3),
    resume_context              JSONB,
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    updated_at                  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_threads_id_is_fleet CHECK (id = fleet_id)
);

-- Serves the fleet's ON DELETE CASCADE. The column never changes after insert, so the index
-- does not block the heap-only update every renewal makes.
CREATE INDEX idx_threads_fleet_id ON thread.threads (fleet_id);

CREATE INDEX idx_threads_last_runner_id_parked
    ON thread.threads (last_runner_id) WHERE park_expires_at IS NOT NULL;

-- The runner alone: every renewal moves `run_expires_at`, and an index on it would
-- block heap-only updates. Serves the foreign key's SET NULL when a runner goes.
CREATE INDEX idx_threads_last_runner_id ON thread.threads (last_runner_id);

GRANT SELECT, INSERT, UPDATE ON thread.threads TO api_runtime;
