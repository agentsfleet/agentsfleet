-- DELETE grants for the runner retention sweep.
--
-- fleet.runner_leases (slot 018) and fleet.runner_events (slot 021) granted
-- api_runtime SELECT/INSERT/UPDATE only — nothing ever deleted rows, and both
-- tables grew without bound. The retention sweeper (serve-tier background
-- work, running as api_runtime) deletes terminal-status rows older than the
-- retention window, so it needs DELETE on exactly these two tables.
--
-- Additive-only: grants, no table or row change.
GRANT DELETE ON fleet.runner_leases TO api_runtime;
GRANT DELETE ON fleet.runner_events TO api_runtime;
