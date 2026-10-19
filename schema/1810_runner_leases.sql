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
-- `fleet_event_id` are opaque values once their rows go. The insert proves them
-- against the fleet and its event in the same statement, and a purge deletes the
-- fleet's ACTIVE leases itself, so no live lease ever names a missing fleet.
-- `posture`, `provider` and `model` are what the lease's usage lines are priced
-- against. `last_metered_at` is the lease's runtime cursor: each renewal, the
-- report and the expiry sweep log the run time since it, on the database's
-- clock, and advance it, never past `lease_expires_at`. `receipt` is the producer's receipt the runner
-- echoes back; `posted_message_count` bounds the messages a lease may post.

CREATE TABLE runner.leases (
    id                          UUID PRIMARY KEY,
    CONSTRAINT ck_leases_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    runner_id                   UUID    NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    fleet_id                    UUID    NOT NULL,
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    fleet_event_id              UUID    NOT NULL,
    event_created_at            TIMESTAMPTZ(3)  NOT NULL,
    receipt                     TEXT    NOT NULL,
    posture                     TEXT    NOT NULL,
    provider                    TEXT    NOT NULL,
    model                       TEXT    NOT NULL,
    last_metered_at             TIMESTAMPTZ(3)  NOT NULL,
    fencing_token               BIGINT  NOT NULL,
    lease_expires_at            TIMESTAMPTZ(3)  NOT NULL,
    status                      TEXT    NOT NULL,
    posted_message_count        INTEGER NOT NULL,
    created_at                  TIMESTAMPTZ(3)  NOT NULL,
    updated_at                  TIMESTAMPTZ(3)  NOT NULL
);

CREATE INDEX idx_leases_fleet_event_id_fencing_token
    ON runner.leases (fleet_event_id, fencing_token);
CREATE INDEX idx_leases_fleet_id_status_fencing_token
    ON runner.leases (fleet_id, status, fencing_token DESC);
-- The runner's lease history and its live leases.
CREATE INDEX idx_leases_runner_id_created_at_id
    ON runner.leases (runner_id, created_at DESC, id DESC);
CREATE INDEX idx_leases_runner_id_status ON runner.leases (runner_id, status);
-- The expiry sweep.
CREATE INDEX idx_leases_status_updated_at ON runner.leases (status, updated_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.leases TO api_runtime;
