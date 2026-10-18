-- Model identities read by admission and pickers. `model_name` is the exact
-- provider API id; `display_name` is its user-facing label. Context evidence is
-- the provider's models API, never a converted marketing label. Dated prices
-- belong in platform_model_prices because one identity has multiple prices.

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
    created_at                   TIMESTAMPTZ(3)  NOT NULL,
    updated_at                   TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_platform_models_provider_model_name UNIQUE (provider, model_name),
    CONSTRAINT ck_platform_models_context_positive CHECK (context_cap_tokens > 0)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_models TO api_runtime;
