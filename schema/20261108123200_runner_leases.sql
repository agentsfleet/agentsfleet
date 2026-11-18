-- One fleet event handed to one runner; an event can be leased more than once, each fenced.
-- No foreign key to fleets or events: a settled lease is history that outlives a fleet purge.
-- The insert proves scope, and a purge deletes the fleet's active leases itself.
CREATE TABLE runner.leases (
    id                          UUID PRIMARY KEY,
    runner_id                   UUID    NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    fleet_id                    UUID    NOT NULL,
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    event_id                    UUID    NOT NULL,
    -- The producer's instant, which billing uses and the event row does not hold.
    event_created_at            TIMESTAMPTZ(3)  NOT NULL,
    posture                     TEXT    NOT NULL,
    provider                    TEXT    NOT NULL,
    model                       TEXT    NOT NULL,
    -- Runtime cursor on the database clock; metering advances it, never past `lease_expires_at`.
    last_metered_at             TIMESTAMPTZ(3)  NOT NULL,
    fencing_token               BIGINT  NOT NULL,
    lease_expires_at            TIMESTAMPTZ(3)  NOT NULL,
    -- Captured at issue; a later deployment profile cannot extend this lease.
    max_runtime_seconds         INTEGER NOT NULL DEFAULT 43200,
    -- Durable Kill decision; a missed notification cannot restore authority.
    cancel_requested_at         TIMESTAMPTZ(3),
    status                      TEXT    NOT NULL,
    posted_message_count        INTEGER NOT NULL,
    created_at                  TIMESTAMPTZ(3)  NOT NULL,
    updated_at                  TIMESTAMPTZ(3)  NOT NULL,
    -- Opaque once the task row is gone; settled leases outlive a fleet purge.
    task_id UUID,
    -- Non-secret snapshot of the decision revision this lease delivered. No foreign key:
    -- settled history must survive catalogue removal.
    decision_snapshot JSONB,
    -- Opaque immutable setup selected at issue; a resumed ordinary root must
    -- match this witness. No foreign key: settled leases survive fleet purge.
    setup_revision_id UUID,
    -- The lease freezes semantic ownership and meters its own task-running delta.
    -- Opaque task_id already outlives task deletion; these add no privacy owner FK.
    task_generation BIGINT CHECK (task_generation > 0),
    task_stop_generation BIGINT CHECK (task_stop_generation >= 0),
    task_started_at TIMESTAMPTZ(3),
    task_runtime_accounted_at TIMESTAMPTZ(3),
    task_execution_deadline_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_leases_event_id_fencing_token
    ON runner.leases (event_id, fencing_token);

CREATE INDEX idx_leases_fleet_id_status_fencing_token
    ON runner.leases (fleet_id, status, fencing_token DESC);

CREATE INDEX idx_leases_runner_id_created_at_id
    ON runner.leases (runner_id, created_at DESC, id DESC);

CREATE INDEX idx_leases_runner_id_status_lease_expires_at
    ON runner.leases (runner_id, status, lease_expires_at);

CREATE INDEX idx_leases_status_updated_at ON runner.leases (status, updated_at);

-- Partial, so it holds only the metering backlog: expired leases not yet billed to expiry.
CREATE INDEX idx_leases_lease_expires_at_unbilled
    ON runner.leases (lease_expires_at)
    WHERE last_metered_at < lease_expires_at;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.leases TO api_runtime;

CREATE INDEX idx_runner_leases_task ON runner.leases (task_id) WHERE task_id IS NOT NULL;
