-- Admits one message in one statement. Rust turns the answer row into
-- 202 / 404 / 409 / 503 (stage/verdict/).
--
-- $1 new row id · $2 fleet · $3 workspace · $4 producer (who sent it) ·
-- $5 dedupe key (a steer's is `<fleet_id>:<operation_id>`) · $6 payload digest ·
-- $7 actor · $8 event type · $9 request JSON · $10 deployment ceiling ·
-- $11 current deployment estimate · $12/$13 reply connector and address ·
-- $14 earlier admission to copy a reply from (task continuations only) ·
-- $15 this build's setup policy stamp · $16 runnable status, or NULL for any ·
-- $17 the active lease status, for the disposition facts (M242_002).

-- 1. Lock the fleet row. One message per fleet is written at a time, so a
--    later sender draws a later sequence. MATERIALIZED: runs once, first.
WITH locked_fleet AS MATERIALIZED (
    SELECT f.id, f.status,
           COALESCE(setup.policy_semantics = $15, false) AS setup_current
    FROM fleet.fleets f
    LEFT JOIN fleet.revisions setup
      ON setup.id = f.head_revision_id AND setup.fleet_id = f.id
    WHERE f.id = $2::uuid AND f.workspace_id = $3::uuid
    FOR NO KEY UPDATE OF f
),
-- 2. Was this key sent before? Read only for a fleet the caller owns.
already_sent AS (
    SELECT a.id::text AS id, a.payload_digest AS digest, a.fleet_id::text AS fleet, a.created_at
    FROM fleet.admissions a, locked_fleet
    WHERE a.producer = $4 AND a.producer_key = $5
),
-- 2b. (M242_002) Is a run of this fleet live, and of which kind? A durable-
--     task run claims a person's later messages in its conversation (#803,
--     pending_inputs.sql); any other run claims every later message
--     (claim_inputs.sql). Facts only; Rust names the disposition.
live_run AS (
    SELECT l.task_id IS NOT NULL AS durable_task,
           head.sequence AS head_sequence,
           head.reply_provider, head.reply_address,
           e.event_id AS run_event_id
    FROM locked_fleet
    JOIN runner.fleet_slots s ON s.fleet_id = locked_fleet.id
    JOIN runner.leases l
      ON l.fleet_id = s.fleet_id AND l.runner_id = s.last_runner_id
     AND l.fencing_token = s.fencing_token AND l.status = $17
    JOIN fleet.events e ON e.id = l.fleet_event_id
    -- Internal setup events have non-UUID keys; only admissions have a head.
    JOIN fleet.admissions head ON head.id = CASE
        WHEN pg_input_is_valid(e.event_id, 'uuid') THEN e.event_id::uuid
    END
    WHERE s.lease_expires_at > statement_timestamp()
),
-- 2c. (M242_002) Is a message waiting that this one must not pass? With no
--     live run, any unsettled message. With one, an unsettled parked or
--     task-bound message after the run's own: claim_inputs.sql's barrier.
earlier_waiting AS (
    SELECT EXISTS (
        SELECT 1 FROM fleet.admissions a, locked_fleet
        WHERE a.fleet_id = locked_fleet.id AND a.settled_at IS NULL
          AND (NOT EXISTS (SELECT 1 FROM live_run)
               OR (a.sequence > (SELECT head_sequence FROM live_run)
                   AND (a.parked_at IS NOT NULL OR a.task_id IS NOT NULL)))
    ) AS waiting
),
-- 3 and 4. Write the message only when every check passes. A twin that
--    committed while this one waited on the lock is returned, not duplicated.
new_message AS (
    INSERT INTO fleet.admissions
      (id, fleet_id, workspace_id, producer, producer_key, payload_digest,
       actor, event_type, request_json, event_created_at,
       reply_provider, reply_address, created_at, updated_at)
    SELECT $1::uuid, locked_fleet.id, $3::uuid, $4, $5, $6, $7, $8, $9, now(),
           COALESCE($12::text, earlier.reply_provider),
           COALESCE($13::text, earlier.reply_address),
           now(), now()
    FROM locked_fleet
    LEFT JOIN fleet.admissions earlier
      ON earlier.id = $14::uuid AND earlier.fleet_id = locked_fleet.id
     AND earlier.reply_provider IS NOT NULL
    WHERE NOT EXISTS (SELECT 1 FROM already_sent)
      AND ($16::text IS NULL OR locked_fleet.status = $16::text)
      AND locked_fleet.setup_current
      AND $11::bigint < $10::bigint
    ON CONFLICT (producer, producer_key) DO UPDATE SET updated_at = EXCLUDED.updated_at
    RETURNING (xmax = 0) AS inserted, id::text AS id, payload_digest AS digest,
              fleet_id::text AS fleet, created_at
)
-- 5. Exactly one answer row, whatever happened above.
SELECT EXISTS (SELECT 1 FROM locked_fleet) AS fleet_found,
       (SELECT status FROM locked_fleet) AS status,
       COALESCE((SELECT setup_current FROM locked_fleet), false) AS setup_current,
       COALESCE(n.inserted, false) AS inserted,
       COALESCE(n.id, s.id) AS id,
       COALESCE(n.digest, s.digest) AS digest,
       COALESCE(n.fleet, s.fleet) AS fleet,
       COALESCE(n.created_at, s.created_at) AS created_at,
       COALESCE((SELECT durable_task FROM live_run), false) AS run_durable_task,
       COALESCE((SELECT reply_provider IS NOT DISTINCT FROM $12::text
                    AND reply_address IS NOT DISTINCT FROM $13::text FROM live_run), false) AS same_conversation,
       (SELECT run_event_id FROM live_run) AS run_event_id,
       (SELECT waiting FROM earlier_waiting) AS earlier_waiting
FROM (SELECT) AS one
LEFT JOIN new_message n ON true
LEFT JOIN already_sent s ON true
