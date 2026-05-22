-- Host-resident runner fleet (zombie-runner split).
-- A runner enrolls via POST /v1/runner/register, exchanging a short-lived
-- enrollment token for a durable per-runner bearer token. The token itself is
-- returned once at register; this table stores only its hash, and the mothership
-- verifies later calls by hashing the presented Bearer (no plaintext token).
--
-- sandbox_tier: isolation strength reported at enrollment
--   (landlock_full | container_nested | macos_seatbelt | dev_none) — values
--   enforced in application code (RULE STS forbids static-string CHECKs).
-- status: runner lifecycle state — app-enforced values, no static CHECK.
-- labels: free-form capability labels, app-supplied JSON array (never NULL).
-- tenant_id: OPTIONAL enrollment scope. NULL = trusted fleet (secrets ship
--   inline over TLS, the only mode wired today). A non-NULL scope reserves the
--   per-tenant-scoped-runner mode so that vision needn't re-cut this table;
--   ON DELETE CASCADE removes a scoped runner when its tenant is deleted.
-- last_seen_at: liveness bookmark, refreshed on heartbeat.

CREATE TABLE IF NOT EXISTS core.runners (
    id            UUID   PRIMARY KEY,
    CONSTRAINT ck_runners_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    host_id       TEXT   NOT NULL,
    token_hash    TEXT   NOT NULL,
    sandbox_tier  TEXT   NOT NULL,
    status        TEXT   NOT NULL,
    labels        JSONB  NOT NULL,
    tenant_id     UUID   NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    last_seen_at  BIGINT NOT NULL,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    CONSTRAINT uq_runners_token_hash UNIQUE (token_hash)
);

-- api_runtime: the serve tier owns /v1/runner (register/heartbeat/lease/report);
-- it inserts at register, updates last_seen_at/status on heartbeat, and reads on
-- every authed call to resolve the runner from its presented token.
GRANT SELECT, INSERT, UPDATE ON core.runners TO api_runtime;
