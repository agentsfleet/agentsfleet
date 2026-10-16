-- Only the daemon writes the cause that authorizes a retry or model replacement.
-- Accounting remains open until all physical dispatches have a known receipt.
ALTER TABLE fleet.authoring_attempts
    ADD COLUMN IF NOT EXISTS daemon_failure TEXT;
