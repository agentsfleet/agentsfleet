-- One unit of work handed to one runner: a fleet's turn. Work can be held more than once, each
-- fenced. A run names its fleet, whose `thread.threads` row it holds, so the statements reach
-- the thread by `fleet_id`. No foreign key to fleets or turns: a settled run is history that
-- outlives its fleet. The insert proves scope, and a fleet's deletion deletes its active runs
-- itself.
-- The runner is history the same way: its deletion, refused while it holds a run or owes
-- metering, leaves its settled runs naming no runner.
CREATE TABLE thread.runs (
    id                          UUID PRIMARY KEY,
    runner_id                   UUID    REFERENCES runner.runners(id) ON DELETE SET NULL,
    fleet_id                    UUID    NOT NULL,
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    -- The fleet turn this run runs.
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
    -- Durable Kill decision; a missed notification cannot restore authority.
    cancel_requested_at         TIMESTAMPTZ(3),
    status                      TEXT    NOT NULL,
    created_at                  TIMESTAMPTZ(3)  NOT NULL,
    updated_at                  TIMESTAMPTZ(3)  NOT NULL,
    -- Opaque once the task row is gone; settled runs outlive a fleet purge.
    durable_task_id UUID,
    -- Non-secret snapshot of the decision revision this run delivered. No foreign key:
    -- settled history must survive catalogue removal.
    decision_snapshot JSONB,
    -- The run freezes semantic ownership and meters its own task-running delta.
    -- Opaque durable_task_id already outlives task deletion; these add no privacy owner FK.
    durable_task_generation BIGINT CHECK (durable_task_generation > 0),
    durable_task_stop_generation BIGINT CHECK (durable_task_stop_generation >= 0),
    durable_task_started_at TIMESTAMPTZ(3),
    durable_task_runtime_accounted_at TIMESTAMPTZ(3),
    durable_task_execution_deadline_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_runs_turn_id_fencing_token
    ON thread.runs (turn_id, fencing_token);

CREATE INDEX idx_runs_fleet_id_status_fencing_token
    ON thread.runs (fleet_id, status, fencing_token DESC);

CREATE INDEX idx_runs_runner_id_created_at_id
    ON thread.runs (runner_id, created_at DESC, id DESC);

CREATE INDEX idx_runs_runner_id_status_run_expires_at
    ON thread.runs (runner_id, status, run_expires_at);

-- Partial, so it holds only the metering backlog: expired runs not yet billed to expiry.
-- Keyed by runner, so a runner's delete asks whether it still owes metering without
-- reading its whole history (`afd_runner/src/sql/runner_admin/delete_runner_if_in_state.sql`).
CREATE INDEX idx_runs_run_expires_at_unbilled
    ON thread.runs (runner_id, run_expires_at)
    WHERE last_metered_at < run_expires_at;

GRANT SELECT, INSERT, UPDATE, DELETE ON thread.runs TO api_runtime;
