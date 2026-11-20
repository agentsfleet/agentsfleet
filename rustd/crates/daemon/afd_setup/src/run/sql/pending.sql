-- Only the newest attempt can still be running, and reading it newest-first keeps the
-- draft's sequence index in charge however long its history.
SELECT true
 WHERE EXISTS (SELECT 1
                 FROM (SELECT a.status, a.message_sequence FROM setup.attempts a
                        WHERE a.draft_id = $1
                        ORDER BY a.message_sequence DESC
                        LIMIT 1) latest
                WHERE latest.status <> ALL($2::text[])
                  AND NOT EXISTS (SELECT 1 FROM thread.turns c
                                   WHERE c.thread_id = $1
                                     AND c.sequence = latest.message_sequence
                                     AND c.status = $3))
    -- A draft run's live slot holds the draft too: its slot's id is the draft's.
    OR EXISTS (SELECT 1 FROM thread.threads s
                WHERE s.id = $1 AND s.run_expires_at > clock_timestamp())
