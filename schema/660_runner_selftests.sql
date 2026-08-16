-- One self-test row per runner: the operator's outstanding request, and the
-- verdict the daemon last reported for it.
--
-- Why: a runner whose sandbox cannot resolve a hostname reports itself healthy.
-- Every check `agentsfleet-runner doctor` runs executes on the HOST, outside the
-- unshared mount namespace a lease actually runs in, so a dangling
-- `/etc/resolv.conf` inside the sandbox is invisible to it — the runner reads
-- ACTIVE·ONLINE while every lease dies on name resolution. This table is the
-- channel that carries an executed in-sandbox proof back to the operator.
--
-- Keyed by its parent rather than carrying its own identity, per the pattern
-- `schema/430_tenant_model_selection.sql` states and `schema/650` first recorded:
-- exactly ONE unique index, because `ON CONFLICT` arbitrates exactly one
-- constraint and a second would make two sessions inserting a brand-new runner's
-- row race to a duplicate-key error on the other index instead of taking the
-- update arm. The request path and the report path both first-touch this row.
--
-- A child table rather than columns on `fleet.runners`: that slot is shipped and
-- frozen history. It also keeps a per-runner JSONB verdict off the row that every
-- authenticated runner call reads to resolve a token.

CREATE TABLE IF NOT EXISTS fleet.runner_selftests (
    runner_id      UUID    PRIMARY KEY REFERENCES fleet.runners(id) ON DELETE CASCADE,

    -- The operator's ask. Set by the dashboard action, cleared by the daemon when
    -- it reports the matching verdict, so "a test is pending" is exactly
    -- `requested_at IS NOT NULL` — no status vocabulary, which RULE STS would
    -- keep out of the schema anyway.
    requested_at   BIGINT,

    -- The verdict. NULL until a first report lands; a runner may hold a request
    -- with no result (never yet reported) or a result with no request (the
    -- startup probe, which no operator asked for).
    completed_at   BIGINT,

    -- The ordered per-check verdicts, each `{name, ok, detail}`. Structural
    -- DEFAULT: an empty array is the identity for "no checks", not a vocabulary
    -- value, so it is not the kind of literal RULE STS bans. It lets the request
    -- path first-touch the row without inventing a verdict.
    checks         JSONB   NOT NULL DEFAULT '[]'::jsonb,

    -- Whether every check passed, decided by the daemon that ran them. Stored
    -- rather than derived so the runner list can filter on it without opening
    -- the JSONB on every row.
    all_ok         BOOLEAN,

    -- The assignment the probe RAN UNDER, not the one in force now. A result
    -- outlives the policy that produced it: re-assigning a runner to
    -- `deny_all_egress` does not re-run its self-test, and rendering the old
    -- verdict as current would tell an operator their new policy is proven when
    -- nothing has tested it. The read compares these two against the live values
    -- on `fleet.runners` and labels a mismatch stale. Plain TEXT, no CHECK
    -- constraint: the tier and policy vocabularies live in application constants
    -- and RULE STS keeps them out of schema objects so the two cannot drift.
    sandbox_tier   TEXT,
    network_policy TEXT,

    created_at     BIGINT  NOT NULL,
    updated_at     BIGINT  NOT NULL
);

-- No index. Both writers and the runner-detail read address this table by
-- `runner_id`, which is the primary key, so the whole access path is indexed.
-- The runner list joins one-to-one on the same key.

-- No uuidv7 CHECK: `runner_id` is minted by `fleet.runners`, whose slot carries
-- the version check.

-- api_runtime: the serve tier owns both sides — the dashboard action writes
-- `requested_at` through /v1/fleets/runners/{id}, and the runner heartbeat path
-- writes the verdict back. No DELETE: the row cascades with its runner, and
-- nothing else may remove a runner's self-test history out from under the page.
GRANT SELECT, INSERT, UPDATE ON fleet.runner_selftests TO api_runtime;
