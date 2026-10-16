-- Provider transport settings belong to each persisted model. No seed or
-- build-time routing fallback: existing rows remain unconfigured until an
-- administrator saves reviewed inference settings through the model API.
-- Credentials stay in vault.secrets; this object contains only API, endpoint,
-- dialect and output limit. Model writes advance model_catalogue_revision so
-- an in-flight authoring attempt cannot silently change its pinned route.
ALTER TABLE core.model_library ADD COLUMN IF NOT EXISTS inference JSONB;
