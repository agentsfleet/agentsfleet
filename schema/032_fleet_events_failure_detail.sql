-- Human-readable failure cause on the event narrative row: which check failed,
-- and why, written by the runner's classification site. NULL on success or when
-- an older runner omits it. Rides the result wire frame and the events envelope
-- verbatim; the canned failure_label sentence stays the headline, this is the
-- cause line beneath it.
--
-- First slot under the additive-migration model: shipped slot files are frozen
-- history, column adds land as new ALTER migrations (SCHEMA_CONVENTIONS.md
-- §Migration Model).

ALTER TABLE core.fleet_events
    ADD COLUMN IF NOT EXISTS failure_detail TEXT NULL;
