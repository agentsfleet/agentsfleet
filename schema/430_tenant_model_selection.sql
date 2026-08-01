-- Tenant-scoped LLM provider configuration. One row per tenant who has
-- explicitly configured a provider; the absence of a row is the synthesised
-- platform default.
--
-- The resolver (state/tenant_provider.zig) treats "no row" and "row with
-- mode = platform" as identical for runtime behaviour. An explicit row is
-- written when the user runs `tenant provider reset`, so the dashboard can
-- distinguish "never configured" from "explicitly reset".
--
-- Identity exception (SCHEMA_CONVENTIONS "Identity Column"), and the first slot
-- to state it, so the later 1:1 tables can cite this one: a row that exists at
-- most once per parent is keyed by the parent. `tenant_id` is both the foreign
-- key and the primary key, named for the parent rather than `id` because that is
-- what the column means and what every statement already filters on.
--
-- This is the shape the retired `fleet.runner_lifetime_counters` slot was
-- reaching for when it shipped a primary key constrained equal to its foreign
-- key: exactly one unique index per table, because `ON CONFLICT` arbitrates
-- exactly one constraint. That slot could not drop the duplicate column under
-- the frozen-slot model, so it added a CHECK tying the two together. Rebuilt
-- from empty, the column simply goes, and the upsert's conflict target IS the
-- primary key.
--
-- Value constraints (mode ∈ {platform, self_managed}; secret_ref nullability
-- tied to mode) are enforced in application code via constants in
-- state/tenant_provider.zig — RULE STS forbids static-string CHECKs.

CREATE TABLE IF NOT EXISTS core.tenant_model_selection (
    tenant_id          UUID    PRIMARY KEY REFERENCES core.tenants(id) ON DELETE CASCADE,
    mode               TEXT    NOT NULL,
    provider           TEXT    NOT NULL,
    model              TEXT    NOT NULL,
    context_cap_tokens INTEGER NOT NULL,
    secret_ref         TEXT,
    created_at         BIGINT  NOT NULL,
    updated_at         BIGINT  NOT NULL
);

-- No uuidv7 CHECK: `tenant_id` is not minted here. It is a foreign key to
-- `core.tenants`, whose own slot carries the version check, so repeating it
-- would re-validate a value the parent already guarantees.

-- No index. Every reader and writer addresses this table by `tenant_id`, which
-- is the primary key, so the table's whole access path is already indexed.
--
-- The retired `idx_tenant_model_selection_mode` is not carried forward. It was
-- created for an operator "list all self-managed tenants" query that exists in
-- no code path, and `mode` holds two values — so it was a btree with no reader
-- over a column that could not select usefully anyway (RULE for §5: an index
-- states the query it serves or it does not exist).

-- api_runtime: GET/PUT/DELETE /v1/tenants/me/provider, plus resolveActiveProvider
-- at lease issue.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.tenant_model_selection TO api_runtime;
