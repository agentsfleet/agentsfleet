-- Runner fleet control plane (agent-runner split). The `fleet` schema holds
-- runner identity and — in later migrations — leases, heartbeats, executions,
-- and fleet-level policy/regions. It is deliberately separate from `core` (the
-- tenant data plane) so the control plane and data plane do not share a trust
-- boundary. This matters most for the open-fleet vision where untrusted hosts
-- enroll: their identity never sits in the tenant-data schema. `fleet` is the
-- system boundary; a runner is one instance within it.
--
-- A runner enrolls via POST /v1/runners, authed by an existing
-- operator/provisioner credential (a Clerk JWT or a zmb_t_ api_key, via
-- bearer_or_api_key — there is no enrollment token). register mints a durable
-- per-runner bearer token (zrn_), returned once; this table stores only its
-- hash, and zombied verifies later calls by hashing the presented Bearer
-- (no plaintext token).
--
-- sandbox_tier: isolation strength reported at register
--   (landlock_full | container_nested | macos_seatbelt | dev_none) — values
--   enforced in application code (RULE STS forbids static-string CHECKs).
-- admin_state: operator intent — active|cordoned|draining|drained|revoked,
--   app-enforced values, no static CHECK (RULE STS). Renamed from `status`.
-- labels: free-form capability labels, app-supplied JSON array (never NULL).
-- tenant_id: OPTIONAL registration scope. NULL = trusted fleet (secrets ship
--   inline over TLS, the only mode wired today). A non-NULL scope reserves the
--   per-tenant-scoped-runner mode so that vision needn't re-cut this table;
--   ON DELETE CASCADE removes a scoped runner when its tenant is deleted.
-- last_seen_at: liveness bookmark, refreshed on heartbeat.

CREATE SCHEMA IF NOT EXISTS fleet;

CREATE TABLE IF NOT EXISTS fleet.runners (
    uid           UUID   GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_runners_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id            UUID   NOT NULL UNIQUE,
    host_id       TEXT   NOT NULL,
    token_hash    TEXT   NOT NULL,
    sandbox_tier  TEXT   NOT NULL,
    admin_state   TEXT   NOT NULL,
    labels        JSONB  NOT NULL,
    tenant_id     UUID   NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    last_seen_at  BIGINT NOT NULL,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    CONSTRAINT uq_runners_token_hash UNIQUE (token_hash)
);

-- api_runtime: the serve tier owns /v1/runners (register/heartbeat/lease/report);
-- it inserts at register, updates last_seen_at on heartbeat, and reads admin_state on
-- every authed call to resolve the runner from its presented token.
GRANT USAGE ON SCHEMA fleet TO api_runtime;
GRANT SELECT, INSERT, UPDATE ON fleet.runners TO api_runtime;
