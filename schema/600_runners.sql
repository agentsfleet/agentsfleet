-- The runner: an enrolled host in the control plane.
--
-- The `fleet` schema is deliberately separate from `core` (the tenant data
-- plane) so the control plane and the data plane do not share a trust boundary.
-- This matters most for the open-fleet direction where untrusted hosts enrol:
-- their identity never sits in the tenant-data schema. `fleet` is the system
-- boundary; a runner is one instance within it.
--
-- A runner enrols via POST /v1/runners, authenticated by an existing operator or
-- provisioner credential — there is no enrolment token. Registration mints a
-- durable per-runner bearer token, returned once; this table stores only its
-- hash, and agentsfleetd verifies later calls by hashing the presented bearer, so
-- no plaintext token is ever stored (RULE VLT).
--
-- Direction of authority for policy: the control plane ASSIGNS policy to a
-- runner row and delivers it with the runner's identity; the runner reports what
-- its kernel can actually enforce; the heartbeat path reconciles the two into a
-- degraded verdict. Assigned and achievable live in separate columns so no code
-- path can overwrite one with the other.
--
--   sandbox_tier        assigned isolation strength (landlock_full |
--                       container_nested | macos_seatbelt | dev_none). The
--                       register handler writes the operator's assignment, never
--                       a host self-report.
--   admin_state         operator intent: active | cordoned | draining | drained
--                       | revoked.
--   labels              free-form capability labels, app-supplied, never NULL.
--   tenant_id           OPTIONAL registration scope. NULL is a trusted fleet
--                       (secrets ship inline over Transport Layer Security, the
--                       only mode wired today). A non-NULL scope reserves the
--                       per-tenant-scoped-runner mode so that direction need not
--                       re-cut this table; the cascade removes a scoped runner
--                       with its tenant.
--   network_policy      assigned egress posture; NULL means no policy assigned
--   registry_allowlist  yet, which the reconciliation treats as degraded and the
--                       runner side fails closed on.
--   worker_count        assigned concurrency for the host's worker pool.
--                       Canonical constant: DEFAULT_WORKER_COUNT
--                       (src/lib/contract/protocol.zig).
--   capability_report   the runner's latest probe result, verbatim, written only
--                       by the heartbeat path. NULL means no report yet, which
--                       is also a degraded state.
--   degraded            the reconciliation verdict and the specific missing
--   degraded_reason     mechanism, both written by the heartbeat path.
--   last_seen_at        liveness bookmark, refreshed on heartbeat.
--
-- All value vocabularies above are app-enforced named constants, never SQL
-- CHECKs (RULE STS).

CREATE TABLE IF NOT EXISTS fleet.runners (
    id                     UUID    PRIMARY KEY,
    CONSTRAINT ck_runners_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    host_id                TEXT    NOT NULL,
    token_hash             TEXT    NOT NULL,
    sandbox_tier           TEXT    NOT NULL,
    admin_state            TEXT    NOT NULL,
    labels                 JSONB   NOT NULL,
    tenant_id              UUID    REFERENCES core.tenants(id) ON DELETE CASCADE,
    network_policy         TEXT,
    registry_allowlist     JSONB,
    worker_count           INTEGER NOT NULL DEFAULT 1,
    capability_report      JSONB,
    capability_reported_at BIGINT,
    degraded               BOOLEAN NOT NULL DEFAULT FALSE,
    degraded_reason        TEXT,
    last_seen_at           BIGINT  NOT NULL,
    created_at             BIGINT  NOT NULL,
    updated_at             BIGINT  NOT NULL,
    -- Every authenticated runner call resolves a presented token to a row by
    -- hash, so this unique constraint is that whole access path as well as its
    -- integrity guarantee: two runners sharing a token hash would make the
    -- resolution ambiguous, which is an authentication bug rather than a data one.
    CONSTRAINT uq_runners_token_hash UNIQUE (token_hash)
);

-- No index on tenant_id. The scoped-runner mode is not wired yet, so the column
-- is NULL on every row today and the cascade it serves has nothing to walk. It
-- gains one when scoped runners ship and the column becomes selective.
--
-- List sorts are deliberately unindexed: runners grow when an operator enrols
-- one — roughly a hundred rows — so sorting them is already free.

-- api_runtime: the serve tier owns /v1/runners (register, heartbeat, lease,
-- report). It inserts at register, updates last_seen_at on heartbeat, reads
-- admin_state on every authenticated call to resolve the runner from its token,
-- and deletes a revoked runner's record at DELETE /v1/fleets/runners/{id}.
--
-- The child tables are not granted DELETE for that path and do not need to be:
-- leases and events cascade from this row and affinity sets NULL, and PostgreSQL
-- executes referential actions as the constraint owner rather than the invoking
-- role. So the append-only privilege posture on fleet.runner_events survives —
-- api_runtime still cannot delete an event row directly.
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.runners TO api_runtime;
