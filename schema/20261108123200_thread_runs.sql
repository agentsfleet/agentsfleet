-- One unit of work handed to one runner: a fleet's turn or a setup draft's turn. Work can
-- be held more than once, each fenced. A run names exactly one owner, the one whose
-- `thread.threads` row it holds, so the statements both kinds share reach the slot by
-- `COALESCE(fleet_id, draft_id)`; `fleet_id` alone is read by the fleet's own statements.
-- No foreign key to fleets, drafts or turns: a settled run is history that outlives its
-- owner. The insert proves scope, and an owner's deletion deletes its active runs itself.
CREATE TABLE thread.runs (
    id                          UUID PRIMARY KEY,
    runner_id                   UUID    NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    fleet_id                    UUID,
    draft_id                    UUID,
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    -- The fleet turn, or the setup draft turn, this run runs.
    turn_id                     UUID    NOT NULL,
    -- The turn's intake instant, copied so billing reads it without a join.
    turn_created_at             TIMESTAMPTZ(3)  NOT NULL,
    posture                     TEXT    NOT NULL,
    provider                    TEXT    NOT NULL,
    model                       TEXT    NOT NULL,
    -- Runtime cursor on the database clock; metering advances it, never past `run_expires_at`.
    last_metered_at             TIMESTAMPTZ(3)  NOT NULL,
    fencing_token               BIGINT  NOT NULL,
    run_expires_at            TIMESTAMPTZ(3)  NOT NULL,
    -- Captured at issue; a later deployment profile cannot extend this run.
    max_runtime_seconds         INTEGER NOT NULL DEFAULT 43200,
    -- Durable Kill decision; a missed notification cannot restore authority.
    cancel_requested_at         TIMESTAMPTZ(3),
    status                      TEXT    NOT NULL,
    posted_message_count        INTEGER NOT NULL,
    created_at                  TIMESTAMPTZ(3)  NOT NULL,
    updated_at                  TIMESTAMPTZ(3)  NOT NULL,
    -- Opaque once the task row is gone; settled runs outlive a fleet purge.
    task_id UUID,
    -- Non-secret snapshot of the decision revision this run delivered. No foreign key:
    -- settled history must survive catalogue removal.
    decision_snapshot JSONB,
    -- Opaque immutable setup selected at issue; a resumed ordinary root must
    -- match this witness. No foreign key: settled runs survive fleet purge.
    setup_revision_id UUID,
    -- The run freezes semantic ownership and meters its own task-running delta.
    -- Opaque task_id already outlives task deletion; these add no privacy owner FK.
    task_generation BIGINT CHECK (task_generation > 0),
    task_stop_generation BIGINT CHECK (task_stop_generation >= 0),
    task_started_at TIMESTAMPTZ(3),
    task_runtime_accounted_at TIMESTAMPTZ(3),
    task_execution_deadline_at TIMESTAMPTZ(3),
    CONSTRAINT ck_runs_one_owner CHECK (num_nonnulls(fleet_id, draft_id) = 1)
);

CREATE INDEX idx_runs_turn_id_fencing_token
    ON thread.runs (turn_id, fencing_token);

CREATE INDEX idx_runs_fleet_id_status_fencing_token
    ON thread.runs (fleet_id, status, fencing_token DESC);

-- The draft kind's lookups by draft, and the live-run delete its deletion runs, as the
-- index above serves the fleet's. Partial: a fleet run has no draft.
CREATE INDEX idx_runs_draft_id_status_fencing_token
    ON thread.runs (draft_id, status, fencing_token DESC) WHERE draft_id IS NOT NULL;

CREATE INDEX idx_runs_runner_id_created_at_id
    ON thread.runs (runner_id, created_at DESC, id DESC);

CREATE INDEX idx_runs_runner_id_status_run_expires_at
    ON thread.runs (runner_id, status, run_expires_at);

CREATE INDEX idx_runs_status_updated_at ON thread.runs (status, updated_at);

-- Partial, so it holds only the metering backlog: expired runs not yet billed to expiry.
CREATE INDEX idx_runs_run_expires_at_unbilled
    ON thread.runs (run_expires_at)
    WHERE last_metered_at < run_expires_at;

GRANT SELECT, INSERT, UPDATE, DELETE ON thread.runs TO api_runtime;

CREATE INDEX idx_runner_runs_task ON thread.runs (task_id) WHERE task_id IS NOT NULL;
