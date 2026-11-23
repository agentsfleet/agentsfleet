-- The turn claim: the head message an assignment took, running from here on, decided in this
-- one statement after the thread claim (`afd_fleet`'s `assign_thread.sql`) won the fence. It
-- matches only a message still unsettled and queued or running (a reassigned one is running
-- already) on a draft still editing. Under Read Committed a claim that waited on the row
-- re-reads the message as committed, so whichever of it and a Stop, a report or a failure
-- commits second sees the first.
--
-- No row: the message was settled or its draft left editing since the assignment read them,
-- and the assignment rolls back, fence and all. A deleted draft took the message with it.
--
-- $1 draft · $2 sequence · $3 the running status · $4 the queued status ·
-- $5 the editing status
UPDATE thread.turns t
   SET status = $3, updated_at = now()
 WHERE t.thread_id = $1 AND t.sequence = $2
   AND t.status IN ($4, $3) AND t.settled_at IS NULL
   AND EXISTS (SELECT 1 FROM setup.drafts d WHERE d.id = t.thread_id AND d.status = $5)
RETURNING t.id
