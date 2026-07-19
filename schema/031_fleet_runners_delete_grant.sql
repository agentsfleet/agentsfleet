-- Let the serve tier delete a revoked runner row.
--
-- Why: migration 017 granted api_runtime only SELECT, INSERT, UPDATE on
-- fleet.runners, because the operator plane was PATCH-only — cordon, drain, and
-- revoke are all state transitions. DELETE /v1/fleets/runners/{id} retires a
-- revoked runner's record, so the role needs the privilege or every delete fails
-- as a permission error surfacing to the operator as a bare 500.
--
-- Child rows are NOT granted here and do not need to be: fleet.runner_leases
-- and fleet.runner_events cascade from the parent, and fleet.runner_affinity
-- sets NULL. Postgres executes referential actions as the constraint owner, not
-- the invoking role, so the append-only privilege posture of migration 021
-- ("No UPDATE/DELETE grant: append-only by privilege") is preserved verbatim —
-- api_runtime still cannot delete an event row directly.

GRANT DELETE ON fleet.runners TO api_runtime;
