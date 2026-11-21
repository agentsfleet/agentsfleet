-- A thread's later save: a compare-and-swap on the version the save expects.
--
-- $1 thread · $2 new version · $3 document · $4 expected version
UPDATE memory.context_checkpoints
   SET resource_version = $2, document = $3, updated_at = now()
 WHERE thread_id = $1 AND resource_version = $4
