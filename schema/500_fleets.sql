-- The fleet: the runtime unit a workspace deploys, and the parent almost every
-- 5xx, 6xx and 8xx row cascades from.
--
--   source_markdown   raw SKILL.md (fleet instructions)
--   trigger_markdown  raw TRIGGER.md (deployment manifest)
--   config_json       server-computed from the trigger frontmatter
--
-- Webhook Hash-based Message Authentication Code (HMAC) secrets live in
-- `vault.secrets` keyed by `fleet:<source>` (or `fleet:<credential_name>` when
-- the trigger frontmatter overrides), so this table holds no secret pointers
-- (RULE VLT). Status transitions are active → paused → active and
-- active → stopped (terminal); the vocabulary is app-enforced, never a CHECK
-- here (RULE STS).
--
-- `required_tags` are the capability tags a fleet needs to be placed (the
-- GitLab-tags / GitHub-labels model). A runner may claim it only when
-- `required_tags` is a subset of that runner's `fleet.runners.labels`. The empty
-- set means any runner, which is today's common case. App-supplied and
-- bounds-validated on create/config (≤32 tags, 1..64 characters each →
-- UZ-REQ-001). Not deduplicated: `<@` containment is set-semantic, so duplicates
-- are harmless. Stored as TEXT[] rather than JSONB because a string set needs no
-- nesting, and only the array `array_ops` Generalized Inverted Index (GIN)
-- operator class supports `<@`.
--
-- `bundle_content_hash` / `bundle_snapshot_key` carry the content identity of the
-- onboarded template a fleet was installed from; the runner materialises the
-- support files from object storage by content hash. No secret values here.
--
-- `workspace_id` cascades. The retired shape had no ON DELETE action, which is
-- why erasing an account needed a hand-maintained delete order that listed this
-- table explicitly — and why a table added later without that line was silently
-- missed. With the cascade, deleting a workspace removes its fleets and, through
-- their own cascades, everything beneath them.

CREATE TABLE IF NOT EXISTS core.fleets (
    id                  UUID   PRIMARY KEY,
    CONSTRAINT ck_fleets_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id        UUID   NOT NULL,
    -- Denormalised from the workspace so a lease can carry the billing tenant
    -- without a join, and tied to it structurally by the composite foreign key
    -- below rather than trusted to the create path.
    tenant_id           UUID   NOT NULL,
    name                TEXT   NOT NULL,
    source_markdown     TEXT   NOT NULL,
    trigger_markdown    TEXT,
    config_json         JSONB  NOT NULL,
    status              TEXT   NOT NULL,
    -- The empty array is the only valid initial value — it is the any-runner
    -- identity, not a vocabulary value — so this DEFAULT is structural rather
    -- than the kind RULE STS bans. The create path always writes the validated
    -- set explicitly; the default only keeps unrelated inserts from re-stating it.
    required_tags       TEXT[] NOT NULL DEFAULT '{}'::text[],
    bundle_content_hash TEXT,
    bundle_snapshot_key TEXT,
    created_at          BIGINT NOT NULL,
    updated_at          BIGINT NOT NULL,
    CONSTRAINT uq_fleets_workspace_id_name UNIQUE (workspace_id, name),
    -- The workspace edge, carrying the tenant with it. Referencing both columns
    -- is what makes `tenant_id` above a fact rather than a copy: a fleet cannot
    -- be created naming one workspace and a different workspace's tenant.
    CONSTRAINT fk_fleets_workspace_id_tenant_id
        FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES core.workspaces (id, tenant_id) ON DELETE CASCADE,
    -- The target `fleet.runner_leases` points at, so a lease's denormalised
    -- workspace and tenant are the fleet's own. Money depends on this: the
    -- settle statement locks the wallet found through the LEASE's tenant_id, so
    -- an unconstrained copy there would debit whichever tenant the lease-issue
    -- path happened to write, and the ledger would record it as legitimate.
    CONSTRAINT uq_fleets_id_workspace_id_tenant_id UNIQUE (id, workspace_id, tenant_id)
);

-- Reader: the runner-placement candidate scan (fleet/sql.zig), which filters
-- `z.required_tags <@ (…)` against the polling runner's labels bound as a
-- constant array — the `column <@ constant` shape GIN can serve. Kept with the
-- caveat the retired slot recorded honestly: `<@` is GIN's weak direction and
-- the empty-set majority is unselective, so confirm with EXPLAIN once placement
-- carries real data.
CREATE INDEX IF NOT EXISTS idx_fleets_required_tags_gin
    ON core.fleets USING gin (required_tags);

-- Reader: the workspace fleets list — the Live Wall's hot path — which is
-- keyset-paged, WHERE workspace_id = $1 ORDER BY created_at DESC, id DESC. The
-- name unique above serves the filter but stops before the sort, so without this
-- index the cursor's (created_at = $2 AND id < $3) tiebreak becomes a post-filter
-- and every page re-sorts the workspace's fleets (RULE KYS, as applied to the
-- tenant charges keyset).
CREATE INDEX IF NOT EXISTS idx_fleets_workspace_id_created_at_id
    ON core.fleets (workspace_id, created_at DESC, id DESC);

-- No other index, and specifically not the retired partial index on
-- `status = 'active'`. Its reader was a Slack routing lookup that no longer
-- exists — today's Slack path resolves a fleet by (workspace_id, name), which
-- the unique constraint above already serves. That left a btree maintained on
-- every fleet write for no query, carrying a bare status literal in its
-- predicate that no application constant corresponded to: a literal in an index
-- predicate names its constant, or the predicate goes.

-- api_runtime creates, reads and updates fleets for the Command-Line Interface
-- install/up/kill operations, and reads config and status at lease-issue time.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleets TO api_runtime;
