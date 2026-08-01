-- Platform default Language Model (LLM) key reference. Stores a pointer
-- (provider → admin workspace) — no key material here (RULE VLT). The real key
-- lives in `vault.secrets` for `source_workspace_id`.
--
-- Key resolution order (runner engine):
--   1. workspace vault.secrets {provider}_api_key       → self-managed
--   2. this table's active row → admin workspace secret → platform default
--   3. WorkerError.CredentialDenied — no environment fallback in any mode
--
-- Identity exception (SCHEMA_CONVENTIONS "Identity Column"): keyed by its
-- natural domain key, like the slug-keyed `core.fleet_library`. The table holds
-- at most one row per provider, and `provider` is how every statement addresses
-- it — the upsert arbitrates ON CONFLICT (provider), both deactivations filter
-- on it, and the admin list orders by it. The retired shape carried a UUID
-- primary key alongside a `UNIQUE (provider)`, and that UUID was written at
-- insert and never selected by anything: not by the admin list, not by the
-- resolver, not by the delete-guard. Making `provider` the primary key removes a
-- mint from the admin write path and leaves the table one unique key, so the
-- upsert's conflict target IS the primary key.
--
-- The active row also carries the priced default it resolves to — model, an
-- optional custom endpoint, and the context cap — so the resolver reads them off
-- this row instead of compile-time constants. Changing the default
-- (PUT /v1/admin/platform-keys) propagates to every platform-mode tenant on
-- their next lease, no redeploy. All three are NULLABLE (deactivation NULLs
-- `model` to release the foreign-key reference); presence is enforced in the
-- app write path and by the foreign key below. No DEFAULT literal and no CHECK
-- list (RULE STS): allowed shapes are app-enforced named constants.
--   model              the priced (provider, model_id) the default resolves to
--   base_url           custom OpenAI-compatible endpoint when the default is not
--                      a named provider; NULL for named providers (built-in
--                      host). Validated https and Server-Side Request
--                      Forgery-safe in the app.
--   context_cap_tokens the context window pinned for the default, mirroring the
--                      catalogue row's cap at activation time.
--
-- `active` carries no DEFAULT. Every writer binds it explicitly — the upsert
-- writes true, both deactivations write false — so a default could only ever
-- fire for a writer that forgot the column, and the row it would mint is an
-- unintentionally-active platform default. Without one, that mistake fails the
-- INSERT instead.

CREATE TABLE IF NOT EXISTS core.platform_provider_defaults (
    provider            TEXT    PRIMARY KEY,
    source_workspace_id UUID    NOT NULL REFERENCES core.workspaces(id),
    active              BOOLEAN NOT NULL,
    model               TEXT,
    base_url            TEXT,
    context_cap_tokens  INTEGER,
    created_at          BIGINT  NOT NULL,
    updated_at          BIGINT  NOT NULL,
    -- Billing-spine integrity: a set (provider, model) MUST be a priced
    -- catalogue row. ON DELETE RESTRICT makes the model-delete vs default-set
    -- race unwinnable — whichever transaction loses fails cleanly, so the active
    -- default can never point at a deleted model, which would panic lease-issue
    -- billing and silently charge the run fee only on renewal. MATCH SIMPLE: a
    -- NULL model is exempt, which is how deactivation releases the reference.
    --
    -- This is the schema's ONE foreign key referencing a domain key rather than
    -- a primary key, because (provider, model_id) is what identifies a priced
    -- model — it holds a different value from `core.model_library.id` rather
    -- than duplicating it, so it is not the twin shape this rebuild retires.
    CONSTRAINT fk_platform_provider_defaults_model
        FOREIGN KEY (provider, model) REFERENCES core.model_library (provider, model_id)
        ON DELETE RESTRICT
);

-- `source_workspace_id` deliberately has no ON DELETE action. A cascade would
-- silently disable the platform default for every tenant the moment an admin
-- workspace was deleted, and SET NULL is not available on a NOT NULL column —
-- so deletion is refused and account erasure removes these rows explicitly
-- (state/account_teardown.zig). This is one of the rows that stays in the
-- explicit delete order precisely because no cascade should cover it.

-- No index on source_workspace_id. The table holds one row per provider — a
-- single-figure row count — so the erasure path's `source_workspace_id IN (…)`
-- delete is a sequential scan over a page, which is cheaper than maintaining an
-- index for it (the growth argument in the retired hot-path-index slot).

-- api_runtime reads and writes via the admin API (PUT/DELETE/GET
-- /v1/admin/platform-keys) and reads during lease issue to resolve the platform
-- default key. DELETE is granted for account erasure.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.platform_provider_defaults TO api_runtime;
