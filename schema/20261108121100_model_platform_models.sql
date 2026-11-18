-- `model_name` is the exact provider API id and never a foreign key; rows point here by `id`.
-- `inference` holds non-secret transport settings. It is NULL until an administrator saves
-- reviewed settings, and there is no routing fallback.
CREATE TABLE model.platform_models (
    id                           UUID PRIMARY KEY,
    CONSTRAINT ck_platform_models_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    model_name                   TEXT    NOT NULL,
    provider                     TEXT    NOT NULL,
    display_name                 TEXT    NOT NULL,
    context_cap_tokens           INTEGER NOT NULL,
    context_source_url           TEXT    NOT NULL,
    context_verified_at          TIMESTAMPTZ(3) NOT NULL,
    advanced                     BOOLEAN NOT NULL,
    inference                    JSONB,
    created_at                   TIMESTAMPTZ(3)  NOT NULL,
    updated_at                   TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_platform_models_provider_model_name UNIQUE (provider, model_name),
    CONSTRAINT ck_platform_models_context_positive CHECK (context_cap_tokens > 0),
    -- `decision_enabled` revokes decision dispatch without touching immutable revisions.
    decision_revision_id UUID,
    decision_enabled BOOLEAN NOT NULL DEFAULT false,
    CONSTRAINT ck_platform_models_decision_activation CHECK (decision_revision_id IS NOT NULL OR NOT decision_enabled)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_models TO api_runtime;
