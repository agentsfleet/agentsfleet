-- Every runner row carries its assigned egress policy and registry baseline.
--
--   fleet.runners.network_policy      (600)  NULL meant "no policy assigned yet"
--   fleet.runners.registry_allowlist  (600)  the same, for the registry list
--
-- Enrolment has always written both (`afd_runner::sql::runner`
-- INSERT_RUNNER_WITH_EVENT), and the operator edit replaces both together, so
-- no writer leaves either NULL. The daemon no longer reads NULL as a state: the
-- heartbeat, the self read and the operator views all require the assignment,
-- and an unreadable one is an error. The constraint makes that a property of
-- the table instead of a promise of every writer.
--
-- `extra_binds` (670) stays nullable: enrolment does not write it, and NULL is
-- its live "no additional binds" value.
--
-- Owner decision, Indy, Oct 16, 2026: no rows from older versions are kept
-- ("There is no legacy ... i will delete all in development and create new").
-- A row holding NULL makes this slot fail rather than be guessed at. VERSION is
-- 0.61.0, above the 0.30.0 anchor, so slot 600 stays frozen history.

ALTER TABLE fleet.runners ALTER COLUMN network_policy SET NOT NULL;
ALTER TABLE fleet.runners ALTER COLUMN registry_allowlist SET NOT NULL;
