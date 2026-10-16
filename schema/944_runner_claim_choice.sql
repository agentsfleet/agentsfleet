-- The host row serializes the choice and insertion of both lease kinds.
-- A stale poll cannot overwrite the alternation established by another daemon.
ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS claim_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_lease_kind TEXT;
