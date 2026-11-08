-- Opens, as received, the event of each message a run folded, joined to the
-- run whose answer it takes: insert_leased_fleet_event.sql for many messages
-- in one statement. Each event's instant is its message's own while the
-- ledger has not stamped the message delivered. An event the message already
-- has keeps its history and learns the run it joined if it named none.
--
-- $1 fleet · $2 the folded messages' ids · $3 the run's event · $4 received status
INSERT INTO fleet.events
  (fleet_id, event_id, workspace_id, actor, event_type,
   status, request_json, resumes_event_id, created_at, updated_at)
SELECT a.fleet_id, a.id, a.workspace_id, a.actor, a.event_type,
       $4, a.request_json::jsonb, $3,
       CASE WHEN a.delivered_at IS NULL THEN a.created_at ELSE now() END, now()
FROM fleet.admissions a
WHERE a.fleet_id = $1 AND a.id = ANY($2)
ON CONFLICT (event_id) DO UPDATE SET
    resumes_event_id = COALESCE(fleet.events.resumes_event_id,
                                EXCLUDED.resumes_event_id)
