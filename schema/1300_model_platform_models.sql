-- The platform model catalogue with prices, read by admission, metering and the
-- model picker. `model_name` is the provider's name for the model and is never a
-- foreign key; rows point here by `id`. Prices are nanos per million tokens.
--
-- `inference` is the model's non-secret transport settings (API, base URL,
-- dialect, output limit), shaped by `afd_wire::inference::ModelInference`. It is
-- NULL until an administrator saves reviewed settings: there is no seed and no
-- routing fallback. Credentials stay in `vault.secrets`.

CREATE TABLE model.platform_models (
    id                           UUID PRIMARY KEY,
    CONSTRAINT ck_platform_models_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    model_name                   TEXT    NOT NULL,
    provider                     TEXT    NOT NULL,
    context_cap_tokens           INTEGER NOT NULL,
    input_nanos_per_mtok         BIGINT  NOT NULL,
    cached_input_nanos_per_mtok  BIGINT  NOT NULL,
    output_nanos_per_mtok        BIGINT  NOT NULL,
    inference                    JSONB,
    created_at                   TIMESTAMPTZ(3)  NOT NULL,
    updated_at                   TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_platform_models_provider_model_name UNIQUE (provider, model_name)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_models TO api_runtime;
