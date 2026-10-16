-- Every owed answer names where it goes.
--
-- Slot 919 added `destination` nullable, because the rows written before it
-- were owed to a model provider and named nowhere; slot 920's scans skipped
-- them by predicate. Those rows can never be delivered, so they are deleted
-- here, and the column carries the rule every writer already keeps
-- (`afd_outbound::obligation`). The two scan indexes are rebuilt on the scans'
-- new predicates under the same names, as slot 920 rebuilt them.
--
-- Owner decision, Indy, Oct 15, 2026. VERSION is 0.61.0, above the
-- 0.30.0 anchor, so slots 913, 919 and 920 stay frozen history.
--
-- A binary that reads `destination` as nullable keeps working against this
-- schema; the one that requires it fails on a NULL this slot removes.

DELETE FROM core.fleet_obligations WHERE destination IS NULL;

ALTER TABLE core.fleet_obligations
    ALTER COLUMN destination SET NOT NULL;

DROP INDEX IF EXISTS core.idx_fleet_obligations_unreceipted;
CREATE INDEX IF NOT EXISTS idx_fleet_obligations_unreceipted
    ON core.fleet_obligations (created_at, seq)
    WHERE receipt IS NULL AND abandoned_at IS NULL;

DROP INDEX IF EXISTS core.idx_fleet_obligations_undelivered;
CREATE INDEX IF NOT EXISTS idx_fleet_obligations_undelivered
    ON core.fleet_obligations (fleet_id, created_at, seq)
    WHERE receipt IS NOT NULL AND delivered_at IS NULL AND abandoned_at IS NULL;
