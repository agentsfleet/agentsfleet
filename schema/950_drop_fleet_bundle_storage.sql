-- The Markdown bundle storage, removed with the format it stored.
--
-- `core.fleet_library` (slot 450) and `core.tenant_fleet_library` (slot 460)
-- held onboarded SKILL.md/TRIGGER.md bundles for the retired import route.
-- The native Library is `core.fleet_libraries` with
-- `core.fleet_library_revisions` (slot 941), which stores the JSON Definition;
-- no foreign key, view or trigger names either old table.
--
-- The four `core.fleets` columns carried the same format per fleet:
-- `source_markdown` and `trigger_markdown` held the raw documents, and
-- `bundle_content_hash` / `bundle_snapshot_key` named the bundle the runner
-- once downloaded. A fleet's setup now lives in
-- `core.fleet_definition_revisions`, and nothing reads these columns.
--
-- Owner decision, Indy, Oct 15, 2026. VERSION is 0.60.0, above the 0.30.0
-- anchor, so slots 450, 460, 500 and 917 stay frozen history.
--
-- `source_markdown` is NOT NULL with no default, so a binary that still writes
-- it fails after this runs, and the binary that omits it fails before. Run
-- `agentsfleetd migrate` in the same rollout that replaces the binary.

DROP TABLE IF EXISTS core.tenant_fleet_library;
DROP TABLE IF EXISTS core.fleet_library;

ALTER TABLE core.fleets
    DROP COLUMN IF EXISTS source_markdown,
    DROP COLUMN IF EXISTS trigger_markdown,
    DROP COLUMN IF EXISTS bundle_content_hash,
    DROP COLUMN IF EXISTS bundle_snapshot_key;
