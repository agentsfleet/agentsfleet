-- Releases a run whose assignment committed but whose frame never left the daemon, so its
-- work goes back to its queue at once instead of lapsing on a runner that never heard of it.
--
-- Fenced three ways: the run must be this runner's, at this fence, and still active, and its
-- thread must still carry that fence. A run a renewal, a sweep or a report has meanwhile
-- settled, or a thread a later assignment has taken, is left untouched and nothing is
-- answered. The lock order is thread, then run, as in every assignment.
--
-- The run is deleted: no runner ever heard of it, so it holds no items, usage or folds, and a
-- run that never left the daemon must not count toward its turn's EVENT_RUNS_MAX (every row
-- of the turn counts, `assign_turn_head.sql` earlier_runs). The thread is freed the way
-- an assignment that issued no run frees it (`release_runless_thread.sql`), so the next
-- assignment takes the same work at the next fence. A fleet's thread is keyed by the fleet,
-- a draft's by the draft, so one statement serves both.
--
-- $1 run · $2 runner · $3 fence · $4 active status
WITH held AS MATERIALIZED (
    SELECT COALESCE(l.fleet_id, l.draft_id) AS thread_id
    FROM thread.runs l
    WHERE l.id = $1 AND l.runner_id = $2 AND l.fencing_token = $3 AND l.status = $4
), thread AS MATERIALIZED (
    SELECT s.id
    FROM thread.threads s
    JOIN held h ON s.id = h.thread_id
    WHERE s.fencing_token = $3
    FOR UPDATE OF s
), released AS (
    DELETE FROM thread.runs l
    USING thread t
    WHERE l.id = $1 AND l.runner_id = $2 AND l.fencing_token = $3 AND l.status = $4
      AND COALESCE(l.fleet_id, l.draft_id) = t.id
    RETURNING t.id AS thread_id
), freed AS (
    UPDATE thread.threads s
    SET run_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
        last_runner_id = CASE WHEN s.hold_expires_at > clock_timestamp() THEN s.last_runner_id END
    FROM released r
    WHERE s.id = r.thread_id AND s.fencing_token = $3
    RETURNING s.id
)
SELECT id AS thread_id FROM freed
