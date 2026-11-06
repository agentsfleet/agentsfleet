-- Monotonic allocation survives deletion of old private waits.
ALTER TABLE fleet.tasks ADD COLUMN wait_sequence BIGINT CHECK (wait_sequence >= 0);
