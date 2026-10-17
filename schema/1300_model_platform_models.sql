-- The platform model catalogue with prices, read by admission, metering and the
-- model picker. `model_name` is the provider's name for the model and is never a
-- foreign key; rows point here by `id`. Prices are nanos per million tokens.

CREATE TABLE model.platform_models (
    id                           UUID PRIMARY KEY,
    CONSTRAINT ck_platform_models_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    model_name                   TEXT    NOT NULL,
    provider                     TEXT    NOT NULL,
    context_cap_tokens           INTEGER NOT NULL,
    input_nanos_per_mtok         BIGINT  NOT NULL,
    cached_input_nanos_per_mtok  BIGINT  NOT NULL,
    output_nanos_per_mtok        BIGINT  NOT NULL,
    created_at                   BIGINT  NOT NULL,
    updated_at                   BIGINT  NOT NULL,
    CONSTRAINT uq_platform_models_provider_model_name UNIQUE (provider, model_name)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_models TO api_runtime;
