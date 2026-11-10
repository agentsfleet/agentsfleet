-- One fleet event handed to one runner. A table, not an events column, because an
-- event can be leased more than once (expiry, reclaim) and each lease is fenced.
--
-- `event_created_at` is the producer's instant billing uses, which the event row
-- does not hold; the lease's actor and event type come from a join to
-- `fleet.events` while that row exists.
--
-- No foreign key to `fleet.fleets` or `fleet.events`: a settled lease is runner
-- history and outlives a fleet purge or an event's retention, for its own 30-day
-- window (deletion matrix: Fleet). `fleet_id`, `workspace_id`, `tenant_id` and
-- `event_id` are opaque values once their rows go. The insert proves them
-- against the fleet and its event in the same statement, and a purge deletes the
-- fleet's ACTIVE leases itself, so no live lease ever names a missing fleet.
-- `posture`, `provider` and `model` are what the lease's usage lines are priced
-- against. `last_metered_at` is the lease's runtime cursor: each renewal, the
-- report and the expiry sweep log the run time since it, on the database's
-- clock, and advance it, never past `lease_expires_at` and never before
-- `created_at`: a runtime usage line is numbered by its slice's start, in
-- milliseconds from the lease's start (M245). `receipt` is the producer's receipt the runner
-- echoes back; `posted_message_count` bounds the messages a lease may post.
CREATE TABLE runner.leases (
    id                          UUID PRIMARY KEY,
    CONSTRAINT ck_leases_id_uuidv7 CHECK (uuid_extract_version(id) IS NOT DISTINCT FROM 7),
    runner_id                   UUID    NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    fleet_id                    UUID    NOT NULL,
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    event_id                    UUID    NOT NULL,
    event_created_at            TIMESTAMPTZ(3)  NOT NULL,
    posture                     TEXT    NOT NULL,
    provider                    TEXT    NOT NULL,
    model                       TEXT    NOT NULL,
    last_metered_at             TIMESTAMPTZ(3)  NOT NULL,
    fencing_token               BIGINT  NOT NULL,
    lease_expires_at            TIMESTAMPTZ(3)  NOT NULL,
    -- Durable Kill decision; a missed notification cannot restore authority.
    cancel_requested_at         TIMESTAMPTZ(3),
    status                      TEXT    NOT NULL,
    posted_message_count        INTEGER NOT NULL,
    created_at                  TIMESTAMPTZ(3)  NOT NULL,
    updated_at                  TIMESTAMPTZ(3)  NOT NULL,
    -- Settled leases outlive a fleet purge; retain only the opaque task identity.
    -- Active lease/task ownership is proven against the current fleet slot and
    -- task row inside the native mutation transaction, as for existing lease data.
    task_id UUID,
    -- Non-secret delivery snapshot of the selected immutable decision revision.
    -- This is the profile delivered by this lease, not a second current selection.
    -- No foreign key: settled runner history must survive catalogue removal.
    -- Physical dispatch must separately prove a live lease, enabled owned profile,
    -- current authority and a native cost reservation before resolving a key.
    decision_snapshot JSONB,
    -- The lease freezes semantic ownership and meters its own task-running delta.
    -- Opaque task_id already outlives task deletion; these add no privacy owner FK.
    task_generation BIGINT CHECK (task_generation > 0),
    task_stop_generation BIGINT CHECK (task_stop_generation >= 0),
    task_started_at TIMESTAMPTZ(3),
    task_runtime_accounted_at TIMESTAMPTZ(3),
    task_execution_deadline_at TIMESTAMPTZ(3),
    CONSTRAINT ck_leases_metered_after_created CHECK (last_metered_at >= created_at)
);

CREATE INDEX idx_leases_event_id_fencing_token
    ON runner.leases (event_id, fencing_token);

CREATE INDEX idx_leases_fleet_id_status_fencing_token
    ON runner.leases (fleet_id, status, fencing_token DESC);

-- The runner's lease history and its live leases.
CREATE INDEX idx_leases_runner_id_created_at_id
    ON runner.leases (runner_id, created_at DESC, id DESC);

-- Reconnect and cancellation refresh read one host's unexpired active leases.
-- The equality prefix still serves other runner/status reads.
CREATE INDEX idx_leases_runner_id_status_lease_expires_at
    ON runner.leases (runner_id, status, lease_expires_at);

-- The expiry sweep.
CREATE INDEX idx_leases_status_updated_at ON runner.leases (status, updated_at);

-- The metering sweep (`select_unbilled_expired_leases.sql`): expired leases
-- whose last slice is not yet billed. Partial, so a lease leaves the index once
-- metering catches up with its expiry, and the index holds only the backlog.
CREATE INDEX idx_leases_lease_expires_at_unbilled
    ON runner.leases (lease_expires_at)
    WHERE last_metered_at < lease_expires_at;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.leases TO api_runtime;

CREATE INDEX idx_runner_leases_task ON runner.leases (task_id) WHERE task_id IS NOT NULL;
