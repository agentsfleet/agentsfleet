-- Claims, for a live run that is not a durable task, every message its fleet
-- received after the run's own, whatever sent it, oldest first. It stops at
-- the first parked message, whose approval gate must not be passed, and the
-- first task-bound one, which belongs to its durable task's generation, so no
-- message passes one sent before it. A message this lease already claimed is
-- returned again, so a retry after a lost answer reads the same messages.
--
-- Only the runner holding the lease claims for it.
--
-- $1 lease · $2 runner · $3 fence · $4 active lease status · $5 read after sequence · $6 limit
WITH run AS MATERIALIZED (
    SELECT l.fleet_id, head.sequence AS head_sequence
    FROM runner.leases l
    JOIN runner.fleet_slots s
      ON s.fleet_id = l.fleet_id AND s.last_runner_id = l.runner_id
     AND s.fencing_token = l.fencing_token
    JOIN fleet.events e ON e.id = l.fleet_event_id AND e.fleet_id = l.fleet_id
    JOIN fleet.admissions head
      ON head.id = CASE
          WHEN pg_input_is_valid(e.event_id, 'uuid') THEN e.event_id::uuid
      END AND head.fleet_id = l.fleet_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
      AND l.fencing_token = $3 AND l.status = $4
      AND l.task_id IS NULL
      AND l.lease_expires_at > clock_timestamp()
      AND s.lease_expires_at > clock_timestamp()
), unsettled AS MATERIALIZED (
    SELECT a.id, a.sequence, (a.parked_at IS NULL AND a.task_id IS NULL) AS foldable
    FROM fleet.admissions a
    JOIN run ON a.fleet_id = run.fleet_id
    WHERE a.settled_at IS NULL AND a.sequence > run.head_sequence
), barrier AS MATERIALIZED (
    -- The first message the run must not fold; nothing after it is claimed.
    SELECT min(sequence) AS sequence FROM unsettled WHERE NOT foldable
), eligible AS (
    SELECT u.id FROM unsettled u CROSS JOIN barrier b
    WHERE u.foldable AND u.sequence > $5
      AND (b.sequence IS NULL OR u.sequence < b.sequence)
    ORDER BY u.sequence
    LIMIT $6
), claimed AS (
    UPDATE fleet.admissions a
    SET input_claim_lease_id = $1::uuid, input_claim_fence = $3, updated_at = now()
    FROM eligible
    WHERE a.id = eligible.id AND a.settled_at IS NULL
    RETURNING a.id, a.sequence, a.request_json
)
SELECT id::text AS id, sequence, request_json FROM claimed ORDER BY sequence
