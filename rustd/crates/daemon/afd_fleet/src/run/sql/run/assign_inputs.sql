-- Assigns, for a live run that is not a durable task, the messages a person
-- sent in the run's conversation after the run's own, oldest first: the
-- durable task's rule (pending_inputs.sql). It stops at the first message of
-- that conversation it must not fold: a parked one, whose approval gate must
-- not be passed; a task-bound one, which belongs to its durable task's
-- generation; and one no person sent, such as a webhook, a schedule fire or a
-- gate's continuation, which runs as a run of its own. Another conversation's
-- messages are neither assigned nor in the way. A message this run already
-- assigned is returned again, so a retry after a lost answer reads the same
-- messages.
--
-- It reads one assignment's worth of the conversation, oldest first, through
-- idx_turns_thread_id_reply_sequence_unsettled, so its cost is the assignment's own however
-- long the fleet's backlog is. The cursor ($5) is the run's own record of what
-- it read: an assignment starts after it.
--
-- Only the runner holding the run assigns for it, and an assignment never replaces
-- a stamp a later fence wrote: fences only rise on a fleet's thread.
--
-- $1 run · $2 runner · $3 fence · $4 active run status · $5 read after sequence · $6 limit ·
-- $7 steer producer · $8 machine actor · $9 person actor pattern · $10 Slack mention producer ·
-- $11 Slack provider · $12 Slack person actor pattern · $13 Slack person actor separator
WITH run AS MATERIALIZED (
    SELECT l.fleet_id, head.sequence AS head_sequence,
           head.reply_provider, head.reply_address
    FROM thread.runs l
    JOIN thread.threads s
      ON s.fleet_id = l.fleet_id AND s.last_runner_id = l.runner_id
     AND s.fencing_token = l.fencing_token
    -- An admitted turn: a run on a history-only receipt has no conversation.
    JOIN thread.turns head
      ON head.id = l.turn_id AND head.thread_id = l.fleet_id AND head.producer IS NOT NULL
    WHERE l.id = $1 AND l.runner_id = $2
      AND l.fencing_token = $3 AND l.status = $4
      AND l.task_id IS NULL AND l.task_generation IS NULL AND head.task_generation IS NULL
      AND l.run_expires_at > clock_timestamp()
      AND s.run_expires_at > clock_timestamp()
), ahead AS MATERIALIZED (
    -- The conversation's next messages: an addressed conversation by equality,
    -- the unaddressed one by NULL, each in the index's own order. The limit
    -- sits inside the lateral, where the planner stops the index walk at it.
    SELECT w.id, w.sequence, w.parked_at, w.task_id, w.task_generation, w.producer, w.actor, w.workspace_id
    FROM run
    CROSS JOIN LATERAL (
        (SELECT a.id, a.sequence, a.parked_at, a.task_id, a.task_generation, a.producer, a.actor, a.workspace_id
         FROM thread.turns a
         WHERE run.reply_provider IS NOT NULL AND a.thread_id = run.fleet_id
           AND a.reply_provider = run.reply_provider AND a.reply_address = run.reply_address
           AND a.settled_at IS NULL AND a.sequence > GREATEST(run.head_sequence, $5)
         ORDER BY a.sequence
         LIMIT $6)
        UNION ALL
        (SELECT a.id, a.sequence, a.parked_at, a.task_id, a.task_generation, a.producer, a.actor, a.workspace_id
         FROM thread.turns a
         WHERE run.reply_provider IS NULL AND a.thread_id = run.fleet_id
           AND a.reply_provider IS NULL AND a.reply_address IS NULL
           AND a.settled_at IS NULL AND a.sequence > GREATEST(run.head_sequence, $5)
         ORDER BY a.sequence
         LIMIT $6)
    ) w
), judged AS (
    -- Foldable until the first message that is not: a running AND in order.
    SELECT f.id, bool_and(f.foldable) OVER (ORDER BY f.sequence) AS clear
    FROM (
        SELECT id, sequence,
               parked_at IS NULL AND task_id IS NULL AND task_generation IS NULL AND CASE
                   WHEN producer = $7 THEN actor <> $8 AND actor LIKE $9
                   WHEN producer = $10 THEN COALESCE(
                       connector.slack_person_allowed(workspace_id, actor, $11, $12, $13), false)
                   ELSE false
               END AS foldable
        FROM ahead
    ) f
), assigned AS (
    UPDATE thread.turns a
    SET input_run_id = $1, input_fence = $3, updated_at = now()
    FROM judged
    WHERE a.id = judged.id AND judged.clear AND a.settled_at IS NULL
      AND (a.input_fence IS NULL OR a.input_fence <= $3)
    RETURNING a.id, a.sequence, a.request_json::text AS request_json
)
SELECT id, sequence, request_json FROM assigned ORDER BY sequence
