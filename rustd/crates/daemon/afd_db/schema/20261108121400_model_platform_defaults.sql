-- The platform default model per provider, and the admin workspace whose vault holds its key.
-- Resolution skips a row whose `active` is false.
CREATE TABLE model.platform_defaults (
    provider             TEXT PRIMARY KEY,
    source_workspace_id  UUID    NOT NULL REFERENCES identity.workspaces(id),
    active               BOOLEAN NOT NULL,
    platform_model_id    UUID REFERENCES model.platform_models(id) ON DELETE RESTRICT,
    base_url             TEXT,
    created_at           TIMESTAMPTZ(3)  NOT NULL,
    updated_at           TIMESTAMPTZ(3)  NOT NULL
);

CREATE INDEX idx_platform_defaults_platform_model_id
    ON model.platform_defaults (platform_model_id) WHERE platform_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_defaults TO api_runtime;
