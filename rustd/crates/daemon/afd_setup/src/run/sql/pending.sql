SELECT true
 WHERE EXISTS (SELECT 1 FROM authoring.attempts
                WHERE draft_id = $1 AND message_sequence > $3
                  AND status <> ALL($2::text[]))
    -- A draft run's live slot holds the draft too: its slot's id is the draft's.
    OR EXISTS (SELECT 1 FROM runner.slots s
                WHERE s.id = $1 AND s.run_expires_at > clock_timestamp())
