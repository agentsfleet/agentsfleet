-- An answer nobody can take is abandoned, and the recovery scans stop offering
-- it.
--
-- Only a delivered verdict stamped an obligation, so a destination that
-- refused an answer for good — a deleted channel, a removed bot — left its row
-- receipted and undelivered, and the undelivered scan re-appended it every
-- `LOST_AFTER` for the life of the fleet. The lanes now stamp such a row
-- abandoned, with a reason, before they acknowledge it: on a permanent refusal,
-- and once a retryable one has spent `MAX_DELIVERY_CYCLES`.
--
-- Both scans also require a destination (slot 919). Every row written before
-- an obligation had to name one was owed to a model provider and can never be
-- delivered; the predicate below is what stops them cycling, with no data
-- rewritten, and they leave with their fleet by cascade.
--
-- `abandon_reason` holds a spelling of `afd_outbound::obligation::AbandonReason`
-- and no literal of it appears here (RULE STS); both-or-neither is a pair of
-- NULL tests. PostgreSQL has no `ADD CONSTRAINT IF NOT EXISTS`, so the slot
-- drops the name it authors first, as slot 916 does.
ALTER TABLE core.fleet_obligations
    ADD COLUMN IF NOT EXISTS abandoned_at   BIGINT,
    ADD COLUMN IF NOT EXISTS abandon_reason TEXT;

ALTER TABLE core.fleet_obligations
    DROP CONSTRAINT IF EXISTS ck_fleet_obligations_abandon_both_or_neither;

ALTER TABLE core.fleet_obligations
    ADD CONSTRAINT ck_fleet_obligations_abandon_both_or_neither
    CHECK ((abandoned_at IS NULL) = (abandon_reason IS NULL));

-- The two scan indexes, rebuilt on the scans' new predicates so each still
-- implies its index (slot 914 records what happens when a query stops doing
-- so). Dropped and re-created under the same names: an index is derived data,
-- and `CREATE INDEX IF NOT EXISTS` alone would keep the old predicate. Built
-- inside the slot's transaction for the reason slot 914 gives.
DROP INDEX IF EXISTS core.idx_fleet_obligations_unreceipted;
CREATE INDEX IF NOT EXISTS idx_fleet_obligations_unreceipted
    ON core.fleet_obligations (created_at, seq)
    WHERE receipt IS NULL AND destination IS NOT NULL AND abandoned_at IS NULL;

DROP INDEX IF EXISTS core.idx_fleet_obligations_undelivered;
CREATE INDEX IF NOT EXISTS idx_fleet_obligations_undelivered
    ON core.fleet_obligations (fleet_id, created_at, seq)
    WHERE receipt IS NOT NULL AND delivered_at IS NULL
      AND destination IS NOT NULL AND abandoned_at IS NULL;
