-- A thread's first save, written only over no row: `ON CONFLICT` leaves a raced first save's.
--
-- $1 id · $2 thread · $3 version · $4 document · $5 expected version
INSERT INTO memory.context_checkpoints
    (id, thread_id, resource_version, document, created_at, updated_at)
SELECT $1, $2, $3, $4, now(), now()
 WHERE $5::bigint = 0
ON CONFLICT DO NOTHING
