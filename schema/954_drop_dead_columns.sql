-- Columns no code writes a value into, dropped with the one index that names
-- one of them.
--
--   billing.authoring_settlements.answer_digest   (940)  never written
--   fleet.authoring_dispatches.provider_continuation (939) only ever cleared
--   core.api_keys.last_used_at                     (240)  never written; the
--                                                         API field went with it
--   core.fleet_events.checkpoint_id                (800)  never written; the
--                                                         API field went with it
--   fleet.runners.tenant_id                        (600)  only ever NULL; its
--                                                         foreign key drops with it
--
-- `idx_authoring_dispatches_private_expiry` (939) is rebuilt on `result` alone,
-- the predicate the privacy sweeper's scan now carries. No view, trigger or
-- other index names any of the five.
--
-- Owner decision, Indy, Oct 15, 2026. VERSION is 0.61.0, above the 0.30.0
-- anchor, so slots 240, 600, 800, 939 and 940 stay frozen history.

DROP INDEX IF EXISTS fleet.idx_authoring_dispatches_private_expiry;

ALTER TABLE fleet.authoring_dispatches DROP COLUMN IF EXISTS provider_continuation;
ALTER TABLE billing.authoring_settlements DROP COLUMN IF EXISTS answer_digest;
ALTER TABLE core.api_keys DROP COLUMN IF EXISTS last_used_at;
ALTER TABLE core.fleet_events DROP COLUMN IF EXISTS checkpoint_id;
ALTER TABLE fleet.runners DROP COLUMN IF EXISTS tenant_id;

CREATE INDEX IF NOT EXISTS idx_authoring_dispatches_private_expiry
    ON fleet.authoring_dispatches (private_expires_at, id)
    WHERE result IS NOT NULL;
