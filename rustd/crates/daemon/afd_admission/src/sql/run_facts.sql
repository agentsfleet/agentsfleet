-- The facts a steer's disposition is named from: whether a run of the fleet
-- is live and of which kind, whether the steer is in that run's conversation,
-- and whether a message the run's claim stops at stands ahead of it. Read
-- after the steer's admission committed and outside the fleet lock, so a send
-- that races a run's start or end can read it one step late; the label is
-- advisory.
--
-- A steer names no reply, so its conversation is the unaddressed one. With no
-- live run, any earlier unsettled message waits ahead of it. With one, the
-- message in the way is claim_inputs.sql's barrier: in that conversation,
-- after the run's own, parked, task-bound or sent by no person. Only the
-- first $7 messages after the run's own are looked at, so the read costs the
-- same however long the backlog is; a barrier past them reads as clear.
--
-- $1 fleet · $2 the steer's admission · $3 active lease status ·
-- $4 steer producer · $5 machine actor · $6 person actor pattern ·
-- $7 messages to look at
WITH message AS MATERIALIZED (
    SELECT a.fleet_id, a.sequence
    FROM fleet.admissions a
    WHERE a.id = $2 AND a.fleet_id = $1
), live_run AS MATERIALIZED (
    SELECT l.task_id IS NOT NULL AS durable_task,
           head.sequence AS head_sequence,
           head.reply_provider IS NULL AND head.reply_address IS NULL AS unaddressed,
           l.event_id AS run_event_id
    FROM message m
    JOIN runner.fleet_slots s ON s.fleet_id = m.fleet_id
    JOIN runner.leases l
      ON l.fleet_id = s.fleet_id AND l.runner_id = s.last_runner_id
     AND l.fencing_token = s.fencing_token AND l.status = $3
    JOIN fleet.admissions head ON head.id = l.event_id
    WHERE s.lease_expires_at > statement_timestamp()
)
SELECT r.run_event_id,
       COALESCE(r.durable_task, false) AS run_durable_task,
       COALESCE(r.unaddressed, false) AS same_conversation,
       CASE WHEN r.run_event_id IS NULL THEN EXISTS (
                SELECT 1 FROM fleet.admissions a
                WHERE a.fleet_id = m.fleet_id AND a.settled_at IS NULL
                  AND a.sequence < m.sequence)
            ELSE EXISTS (
                SELECT 1 FROM (
                    SELECT a.parked_at, a.task_id, a.producer, a.actor
                    FROM fleet.admissions a
                    WHERE a.fleet_id = m.fleet_id
                      AND a.reply_provider IS NULL AND a.reply_address IS NULL
                      AND a.settled_at IS NULL
                      AND a.sequence > r.head_sequence AND a.sequence < m.sequence
                    ORDER BY a.sequence
                    LIMIT $7
                ) ahead
                WHERE NOT (ahead.parked_at IS NULL AND ahead.task_id IS NULL
                           AND ahead.producer = $4 AND ahead.actor <> $5
                           AND ahead.actor LIKE $6))
       END AS earlier_waiting
FROM message m
LEFT JOIN live_run r ON true
