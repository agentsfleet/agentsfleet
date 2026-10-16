-- The fleet is the retained setup identity; private deletion cannot erase it.
ALTER TABLE core.fleets ADD COLUMN IF NOT EXISTS setup_context JSONB;
ALTER TABLE core.fleets ADD COLUMN IF NOT EXISTS setup_stream_ready BOOLEAN;
ALTER TABLE core.fleets ADD COLUMN IF NOT EXISTS initial_start JSONB;
ALTER TABLE core.fleets ADD COLUMN IF NOT EXISTS resource_version BIGINT;
ALTER TABLE core.fleets ADD COLUMN IF NOT EXISTS setup_runtime_model_ref TEXT;
ALTER TABLE core.fleet_definition_revisions ADD COLUMN IF NOT EXISTS runtime_model_ref TEXT;
