-- An enrolled host: its assignment, its health and its last self-test. Runners
-- hold no datastore credential; agentsfleetd reads and writes this row for them.
--
-- Assignment (read on every heartbeat and claim): `sandbox_tier`, `labels`,
-- `network_policy`, `registry_allowlist`, `worker_count`, and `sandbox_config` —
-- tier-specific knobs as JSON validated in the application (`{"extra_binds": [...]}`
-- for namespace tiers). Health: degraded is `degraded_reason IS NOT NULL`, derived
-- on read. Self-test pass is derived from `selftest_checks`, keeping "no self-test
-- yet" distinct from "self-test failed".
--
-- `lease_version` is the compare-and-swap counter that serializes the choice of
-- the next lease type on this host; `last_lease_type` is the type it chose last,
-- so a stale poll cannot overwrite the alternation another daemon established.

CREATE TABLE runner.runners (
    id                       UUID PRIMARY KEY,
    CONSTRAINT ck_runners_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    host_id                  TEXT    NOT NULL,
    token_hash               TEXT    NOT NULL,
    sandbox_tier             TEXT    NOT NULL,
    status                   TEXT    NOT NULL,
    labels                   JSONB   NOT NULL,
    network_policy           TEXT    NOT NULL,
    registry_allowlist       JSONB   NOT NULL,
    worker_count             INTEGER NOT NULL,
    sandbox_config           JSONB,
    capability_report        JSONB,
    degraded_reason          TEXT,
    last_seen_at             TIMESTAMPTZ(3)  NOT NULL,
    selftest_requested_at    TIMESTAMPTZ(3),
    selftest_completed_at    TIMESTAMPTZ(3),
    selftest_checks          JSONB,
    selftest_sandbox_tier    TEXT,
    selftest_network_policy  TEXT,
    lease_version            BIGINT  NOT NULL,
    last_lease_type          TEXT,
    created_at               TIMESTAMPTZ(3)  NOT NULL,
    updated_at               TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_runners_token_hash UNIQUE (token_hash)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.runners TO api_runtime;
