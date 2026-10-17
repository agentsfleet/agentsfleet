-- One fleet event handed to one runner. A table, not an events column, because an
-- event can be leased more than once (expiry, reclaim) and each lease is fenced.
--
-- `event_created_at` is the producer's instant billing uses, which the event row
-- does not hold; the lease's actor and event type come from a join to
-- `fleet.events`. `posture`, `provider`, `model` and the `metered_*` counts are
-- what renewal charges against. `receipt` is the producer's receipt the runner
-- echoes back; `posted_message_count` bounds the messages a lease may post.

CREATE TABLE runner.leases (
    id                          UUID PRIMARY KEY,
    CONSTRAINT ck_leases_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    runner_id                   UUID    NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    fleet_id                    UUID    NOT NULL,
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    fleet_event_id              UUID    NOT NULL REFERENCES fleet.events(id) ON DELETE CASCADE,
    event_created_at            BIGINT  NOT NULL,
    receipt                     TEXT    NOT NULL,
    posture                     TEXT    NOT NULL,
    provider                    TEXT    NOT NULL,
    model                       TEXT    NOT NULL,
    metered_input_token_count   BIGINT  NOT NULL,
    metered_cached_token_count  BIGINT  NOT NULL,
    metered_output_token_count  BIGINT  NOT NULL,
    fencing_token               BIGINT  NOT NULL,
    lease_expires_at            BIGINT  NOT NULL,
    status                      TEXT    NOT NULL,
    posted_message_count        INTEGER NOT NULL,
    created_at                  BIGINT  NOT NULL,
    updated_at                  BIGINT  NOT NULL,
    -- Proves the fleet's workspace and tenant, and cascades a purge.
    CONSTRAINT fk_leases_fleet_workspace_tenant
        FOREIGN KEY (fleet_id, workspace_id, tenant_id)
        REFERENCES fleet.fleets (id, workspace_id, tenant_id) ON DELETE CASCADE
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
