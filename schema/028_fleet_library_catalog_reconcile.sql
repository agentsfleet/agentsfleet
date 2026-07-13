-- Reconciles the first-party catalog on a database that already applied 023.
--
-- HISTORICAL (M127), kept deliberately. It is registered and already applied in
-- production, so it stays in the migration array: versions must be contiguous
-- (`cmd/common.zig` asserts `last version == registered count`), and this file is
-- a truthful record of what a deployed database has run. On a FRESH database it is
-- a no-op — 023 no longer seeds, so there is no row here to reconcile.
--
-- Its seed INSERT was removed by M128, which made the catalog runtime-owned: a
-- fleet is born when an operator adds a repository from /admin/fleet-libraries,
-- never in SQL (M128 Invariant 5). Re-seeding here would have re-created those four
-- rows on every fresh database and defeated that outright. The DELETE below stays
-- because it remains true and costs nothing.
--
-- Data only: no ALTER, no DROP, no schema change. Idempotent.

-- `security-reviewer` named a repository that was never published, so its row
-- could only ever fail an onboard. Scoped to a row holding no bundle: a non-null
-- content_hash would mean real content landed here, and this delete must never
-- destroy one.
DELETE FROM core.fleet_library
 WHERE id = 'security-reviewer'
   AND content_hash IS NULL;
