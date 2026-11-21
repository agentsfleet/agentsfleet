-- The one checkpoint a thread owns, absent before its first save.
--
-- $1 thread
SELECT document FROM memory.context_checkpoints
 WHERE thread_id = $1
