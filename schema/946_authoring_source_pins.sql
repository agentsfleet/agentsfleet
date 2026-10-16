-- Source identity and instance selection are separate from mutable chat titles.
ALTER TABLE core.authoring_drafts ADD COLUMN IF NOT EXISTS source_pin JSONB;
ALTER TABLE core.authoring_drafts ADD COLUMN IF NOT EXISTS instance_name TEXT;
