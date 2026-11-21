-- Whether the draft's thread holds a live run, read under the draft's lock: one live run
-- per draft, and its thread's id is the draft's.
--
-- $1 draft
SELECT EXISTS (SELECT 1 FROM thread.threads s
                WHERE s.id = $1 AND s.run_expires_at > clock_timestamp())
