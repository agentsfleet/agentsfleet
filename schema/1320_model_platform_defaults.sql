-- The platform default model per provider and the admin workspace whose vault
-- holds its key. Keyed by `provider` (an identity exception): every statement
-- addresses a row by provider, and there is at most one per provider.
--
-- `base_url` is the custom endpoint the platform default calls; `active` is read
-- by resolution, which skips an inactive provider. The model is a pointer into the
-- catalogue, so deleting a model still in use is refused.

CREATE TABLE model.platform_defaults (
    provider             TEXT PRIMARY KEY,
    source_workspace_id  UUID    NOT NULL REFERENCES identity.workspaces(id),
    active               BOOLEAN NOT NULL,
    platform_model_id    UUID REFERENCES model.platform_models(id) ON DELETE RESTRICT,
    base_url             TEXT,
    created_at           BIGINT  NOT NULL,
    updated_at           BIGINT  NOT NULL
);

CREATE INDEX idx_platform_defaults_platform_model_id
    ON model.platform_defaults (platform_model_id) WHERE platform_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_defaults TO api_runtime;
