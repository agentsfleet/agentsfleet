-- Additive migration: one actor's events in one fleet, read by index.
--
-- Reader: a running fleet's schedule run list (`afd_events::history::actor`),
-- which pages `WHERE workspace_id = $1 AND fleet_id = $2 AND actor = $3`
-- newest-first. The actor is `cron:<schedule_id>`, so the rows it matches are
-- sparse: a schedule fires a few times a day among every steer and webhook the
-- fleet takes, and a `once` schedule fires once. Walking
-- `idx_fleet_events_fleet_id_created_at_event_id` (schema/800) and filtering on
-- the actor reads the fleet's whole history to fill one page, and a model can
-- ask for that page in any run. schema/800's note on actor filters assumed
-- dense matches such as the dashboard's `cron:*`; a single schedule is not.
--
-- With the actor between the fleet and the keyset columns, both equalities are
-- index conditions, the order comes off the index, and `LIMIT` stops the read
-- at one page. The trailing `event_id` is the tiebreak (RULE KYS).
--
-- An index and nothing else: no table, column or grant changes. VERSION is
-- 0.56.0, above the 0.30.0 anchor, so schema/800 stays frozen history.
-- Idempotent (IF NOT EXISTS) for a fresh bootstrap and an already-provisioned
-- database alike.

CREATE INDEX IF NOT EXISTS idx_fleet_events_fleet_id_actor_created_at_event_id
    ON core.fleet_events (fleet_id, actor, created_at DESC, event_id DESC);
