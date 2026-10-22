-- The replay reader uses the physical dispatch's recorded deadline. Old calls
-- used a thirty-second transport bound; preserve that bound during upgrade.
-- failure_cause is a content-free explanation read by private message pages.
ALTER TABLE authoring.dispatches
    ADD COLUMN dispatch_expires_at TIMESTAMPTZ(3),
    ADD COLUMN failure_cause TEXT;

UPDATE authoring.dispatches d
   SET dispatch_expires_at = LEAST(a.expires_at, d.created_at + INTERVAL '30 seconds')
  FROM authoring.attempts a
 WHERE a.id = d.attempt_id;

ALTER TABLE authoring.dispatches
    -- The previous daemon can still insert during a rolling replacement.
    -- Its retired transport bound was thirty seconds. New writers always
    -- supply their explicit deadline, capped by the attempt's expiry.
    ALTER COLUMN dispatch_expires_at SET DEFAULT (clock_timestamp() + make_interval(secs => 30)),
    ALTER COLUMN dispatch_expires_at SET NOT NULL;
