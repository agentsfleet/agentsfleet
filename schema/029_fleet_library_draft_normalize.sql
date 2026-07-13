-- Normalizes a deployed catalog onto the M128 publish lifecycle.
--
-- Before M128, every core.fleet_library row was stored 'public' the moment it
-- existed — including the four seed rows that carried no bundle at all. That is
-- the accident M128 ends: 'public' now means "live in every tenant's gallery",
-- and a row with no content_hash has nothing to serve. A bundle-less row that
-- claims to be public would be advertised by GET /v1/fleets/bundles and then
-- fail to resolve on install.
--
-- 023 is the canonical DDL and was corrected in place (its seed deleted), per
-- the pre-v2.0.0 teardown-rebuild convention (docs/SCHEMA_CONVENTIONS.md §1).
-- That convention only reaches a database built from scratch: the migrator
-- records applied versions in audit.schema_migrations and compares by version
-- number with no checksum over the SQL text, so an edited 023 is skipped
-- wherever version 23 already ran (the lesson M127 learned the hard way). This
-- file is how the correction reaches an existing deployment.
--
-- Data only: no ALTER, no DROP, no schema change (check-schema-gate forbids them
-- below v2.0.0). Idempotent, and a no-op on a fresh database — 023 no longer
-- seeds, so there is nothing here to normalize.
--
-- Supersedes 028_fleet_library_catalog_reconcile.sql, which is deleted in the
-- same change: its only job was reconciling the seed, and with the seed gone it
-- would re-insert those four rows into a fresh database. Version 28 stays
-- recorded on databases that already applied it; the migrator resolves by
-- applied-version set, so it is simply never re-run.

-- A row with no bundle cannot be published. Scoped to content_hash IS NULL: a
-- row that HOLDS a bundle and is public is genuinely published, and this must
-- never withdraw one. Canonical constant: VISIBILITY_DRAFT in
-- src/agentsfleetd/fleet_library/library_store.zig — a test asserts the two
-- agree, because SQL cannot import the Zig constant (RULE STS).
UPDATE core.fleet_library
   SET visibility = 'draft',
       updated_at = (extract(epoch from now()) * 1000)::bigint
 WHERE content_hash IS NULL
   AND visibility <> 'draft';
