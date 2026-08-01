-- The per-event narrative log: one row per delivery into a fleet's event stream,
-- and the highest-insert table in the system.
--
-- Mutable, insert-then-update:
--   INSERT at start (status = received)                      write path step 2
--   UPDATE on gate  (status = gate_blocked)                  write path step 4
--   UPDATE at end   (status = processed | fleet_error)       write path step 9
--
-- Idempotent on replay through UNIQUE (fleet_id, event_id) plus
-- ON CONFLICT DO NOTHING. That constraint is also what `fleet/reclaim.zig` joins
-- on to read an expired lease's event body, now that the lease no longer carries
-- its own copy — both tables cascade from the same fleet, so the
-- join cannot dangle.
--
-- Status and event-type vocabularies are app-enforced named constants; a CHECK
-- with literal strings drifts silently from them (RULE STS).
--
-- `request_json` and `response_text` stay HERE — they are the event, and this is
-- where it lives. What changes is that the list read stops selecting them:
-- rendering a table of timestamps, statuses and costs shipped up
-- to two hundred full payloads and two hundred full agent answers per page.
-- Oversized-attribute storage already keeps the wide values out of the row, so
-- the list was slow because it SELECTED them, not because they existed. The
-- single-event detail read is what the expand interaction calls instead.
--
-- `failure_label` is the normalised failure cause and `failure_detail` the
-- human-readable line beneath it, both written by the runner's classification
-- site and both NULL on success.

CREATE TABLE IF NOT EXISTS core.fleet_events (
    id               UUID   PRIMARY KEY,
    CONSTRAINT ck_fleet_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id         UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    workspace_id     UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    event_id         TEXT   NOT NULL,
    actor            TEXT   NOT NULL,
    event_type       TEXT   NOT NULL,
    status           TEXT   NOT NULL,
    request_json     JSONB  NOT NULL,
    response_text    TEXT,
    tokens           BIGINT,
    wall_ms          BIGINT,
    failure_label    TEXT,
    failure_detail   TEXT,
    checkpoint_id    TEXT,
    resumes_event_id TEXT,
    created_at       BIGINT NOT NULL,
    updated_at       BIGINT NOT NULL,
    -- The replay idempotency key, and the join reclaim reads the body through.
    CONSTRAINT uq_fleet_events_fleet_id_event_id UNIQUE (fleet_id, event_id)
);

-- Reader: the per-fleet history page, newest-first, cursor-paged on the
-- (created_at, event_id) tuple. Actor-filtered reads ride the same index by
-- seeking on fleet and filtering as they scan; with a LIMIT of 50 and
-- most-recent-first ordering that fills a page in a few reads even on chatty
-- fleets. If actor filtering ever becomes a measured bottleneck, an expression
-- or partial index can be added then.
CREATE INDEX IF NOT EXISTS idx_fleet_events_fleet_id_created_at_event_id
    ON core.fleet_events (fleet_id, created_at DESC, event_id DESC);

-- Reader: the workspace-aggregate history page, same keyset shape one level up.
-- The trailing `event_id` is the tiebreak (RULE KYS): without it the
-- (created_at = $2 AND event_id < $3) predicate became a post-filter on every
-- page, which is what the retired two-column form did. Its `workspace_id` prefix
-- also serves the workspace cascade.
CREATE INDEX IF NOT EXISTS idx_fleet_events_workspace_id_created_at_event_id
    ON core.fleet_events (workspace_id, created_at DESC, event_id DESC);

-- Reader: continuation-chain walks — context-chunk continuations and
-- gate-resolved re-enqueue. Partial because only continuation rows carry the
-- column, and the predicate is a NULL test rather than a value literal, so no
-- application constant is mirrored here (RULE STS).
CREATE INDEX IF NOT EXISTS idx_fleet_events_fleet_id_resumes_event_id
    ON core.fleet_events (fleet_id, resumes_event_id)
    WHERE resumes_event_id IS NOT NULL;

-- api_runtime writes the lifecycle in the lease and report paths, and serves the
-- read endpoints: the per-fleet list, the workspace aggregate, the single-event
-- detail read, and the server-sent-events backfill.
GRANT SELECT, INSERT, UPDATE ON core.fleet_events TO api_runtime;
