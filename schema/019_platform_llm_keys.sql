-- M16_004 §1.1: Platform default LLM key reference table.
-- Stores a pointer (provider → admin workspace) — no key material here.
-- The real key lives in vault.secrets for source_workspace_id.
-- Key resolution order (worker_stage_executor.zig):
--   1. workspace vault.secrets {provider}_api_key  → BYOK
--   2. platform_llm_keys active row → admin workspace vault.secrets  → platform default
--   3. WorkerError.CredentialDenied — no env fallback in any mode

CREATE TABLE core.platform_llm_keys (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider            TEXT NOT NULL,
    source_workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    active              BOOLEAN NOT NULL DEFAULT true,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_platform_llm_keys_provider UNIQUE (provider)
);

-- worker_runtime reads during run execution to resolve platform default key.
-- api_runtime reads/writes via admin API (PUT/DELETE/GET /v1/admin/platform-keys).
GRANT SELECT ON core.platform_llm_keys TO worker_runtime;
GRANT SELECT, INSERT, UPDATE ON core.platform_llm_keys TO api_runtime;
