-- Prove `apply.sql` landed. Read-only; safe to run any number of times.
--
--   psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f verify.sql
--
-- Four rows, every one expected to read PASS. Any FAIL means the database does
-- not match the schema this milestone ships, and the application will disagree
-- with it — the billing read selects columns by name and the purge relies on the
-- cascade.

\set ON_ERROR_STOP on

-- 1. The wallet column is gone.
SELECT
    'wallet has no free_trial_ends_at' AS check,
    CASE WHEN count(*) = 0 THEN 'PASS' ELSE 'FAIL' END AS result,
    count(*) AS found
FROM information_schema.columns
WHERE table_schema = 'billing'
  AND table_name   = 'tenant_wallet'
  AND column_name  = 'free_trial_ends_at'

UNION ALL

-- 2. The foreign key exists, under the name schema/820 declares.
SELECT
    'memory_entries has fk_memory_entries_fleet_id',
    CASE WHEN count(*) = 1 THEN 'PASS' ELSE 'FAIL' END,
    count(*)
FROM pg_constraint
WHERE conname = 'fk_memory_entries_fleet_id'
  AND conrelid = 'memory.memory_entries'::regclass
  AND contype = 'f'

UNION ALL

-- 3. It cascades. A foreign key with the right name but ON DELETE NO ACTION
--    would pass check 2 and still leave every memory row behind on erasure,
--    which is the whole defect — so the delete action is asserted, not assumed.
SELECT
    'the fleet edge cascades on delete',
    CASE WHEN count(*) = 1 THEN 'PASS' ELSE 'FAIL' END,
    count(*)
FROM pg_constraint
WHERE conname = 'fk_memory_entries_fleet_id'
  AND conrelid = 'memory.memory_entries'::regclass
  AND confdeltype = 'c'

UNION ALL

-- 4. No orphans survived. Zero by construction once the constraint validates,
--    asserted anyway so a database that acquired the key some other way is
--    still checked.
SELECT
    'no memory rows without a fleet',
    CASE WHEN count(*) = 0 THEN 'PASS' ELSE 'FAIL' END,
    count(*)
FROM memory.memory_entries m
WHERE NOT EXISTS (SELECT 1 FROM core.fleets f WHERE f.id = m.fleet_id);
