-- The one checkpoint a thread owns, absent before its first save.
--
-- $1 thread
SELECT document FROM thread.compactions
 WHERE thread_id = $1
