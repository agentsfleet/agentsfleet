-- Platform default LLM key reference table.
-- Stores a pointer (provider → admin workspace) — no key material here.
-- The real key lives in vault.secrets for source_workspace_id.
-- Key resolution order (runner engine):
--   1. workspace vault.secrets {provider}_api_key  → self-managed
--   2. platform_llm_keys active row → admin workspace vault.secrets  → platform default
--   3. WorkerError.CredentialDenied — no env fallback in any mode

-- The active row also carries the priced default it resolves to — model, an
-- optional custom endpoint, and the context cap — so the resolver reads them
-- straight off this row instead of compile-time constants. Changing the default
-- (PUT /v1/admin/platform-keys) propagates to every platform-mode tenant on
-- their next lease, no redeploy. All three are NULLABLE (a row may predate a
-- proper default-set); presence is enforced in the app write path (the admin PUT
-- validates `model` is a priced core.model_library row before activating) AND, since
-- M100, by the fk_platform_llm_keys_model FK below — the DB makes the model-delete
-- vs default-set race unwinnable so the active default can never reference a
-- deleted catalogue row. No DEFAULT literal / no CHECK list (RULE STS): the
-- allowed shapes are app-enforced named constants, not frozen SQL.
--   model              the priced (provider, model_id) the default resolves to
--   base_url           custom OpenAI-compatible endpoint when the default is not
--                      a named provider; NULL for named providers (built-in
--                      host). Validated https + SSRF-safe in the app.
--   context_cap_tokens the context window pinned for the default, mirroring the
--                      catalogue row's cap at activation time.
CREATE TABLE IF NOT EXISTS core.platform_llm_keys (
    uid                 UUID GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_platform_llm_keys_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id                  UUID NOT NULL UNIQUE,
    provider            TEXT NOT NULL,
    source_workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    active              BOOLEAN NOT NULL DEFAULT true,
    model               TEXT,
    base_url            TEXT,
    context_cap_tokens  INTEGER,
    created_at          BIGINT NOT NULL,
    updated_at          BIGINT NOT NULL,
    CONSTRAINT uq_platform_llm_keys_provider UNIQUE (provider),
    -- Billing-spine integrity: a set (provider, model) MUST be a priced catalogue
    -- row. ON DELETE RESTRICT makes the model-delete vs default-set race
    -- unwinnable — whichever txn loses fails cleanly, so the active default can
    -- never point at a deleted model (which would panic lease-issue billing and
    -- silently run-fee-only on renewal). MATCH SIMPLE: a NULL model is exempt, so
    -- deactivation NULLs model to release this reference.
    CONSTRAINT fk_platform_llm_keys_model
        FOREIGN KEY (provider, model) REFERENCES core.model_library (provider, model_id)
        ON DELETE RESTRICT
);

-- api_runtime reads/writes via admin API (PUT/DELETE/GET /v1/admin/platform-keys)
-- and reads during lease issue to resolve the platform default key.
GRANT SELECT, INSERT, UPDATE ON core.platform_llm_keys TO api_runtime;
