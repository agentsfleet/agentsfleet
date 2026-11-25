-- The turn a create wrote, by the key it admitted the message under.
--
-- $1 fleet · $2 producer · $3 producer key
SELECT id FROM thread.turns
 WHERE thread_id = $1 AND producer = $2 AND producer_key = $3
