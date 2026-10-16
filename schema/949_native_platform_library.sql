-- Native entries retain UUID revision links. Only the curated platform tier
-- has a stable public slug; tenant entries continue to expose their UUID.
-- The slug is a distinct natural name, never a duplicate UUID identity.
ALTER TABLE core.fleet_libraries ADD COLUMN IF NOT EXISTS platform_slug TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_fleet_libraries_platform_slug
    ON core.fleet_libraries(platform_slug) WHERE platform_slug IS NOT NULL;

-- Operator-owned copy survives later candidate revisions. A null uses the
-- authored Definition's reasons rather than manufacturing empty metadata.
ALTER TABLE core.fleet_libraries ADD COLUMN IF NOT EXISTS credential_reasons JSONB;
