-- Activation's own budget governs its waits, not the API login's bounds
-- (M242_001 §6): $1 sits just past SETUP_BUDGET, so the budget answers first
-- and the server still frees a wait whose client is gone.
SELECT set_config('lock_timeout', $1, true), set_config('statement_timeout', $1, true)
