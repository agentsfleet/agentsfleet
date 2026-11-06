-- A task transaction's own bounds win over the API login's (M242_001 §6):
-- the statement bound governs, and lock waits are left to it, as before §6.
SELECT set_config('statement_timeout', $1, true), set_config('lock_timeout', $2, true)
