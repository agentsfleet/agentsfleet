-- Seek through the primary key before examining any owner's generations or pins.
WITH page AS MATERIALIZED (
    SELECT event_id FROM runner.turn_recovery
    WHERE event_id > COALESCE($3::uuid, UUID '00000000-0000-0000-0000-000000000000')
    ORDER BY event_id LIMIT $2
)
SELECT h.event_id, a.fleet_id, a.workspace_id,
       COALESCE(
           (a.settled_at <= statement_timestamp() - make_interval(secs => $1)
            AND NOT EXISTS (
                SELECT 1 FROM runner.turn_checkpoints p
                JOIN runner.turn_checkpoint_pins pin ON pin.turn_checkpoint_id = p.id
                WHERE p.event_id = h.event_id AND pin.released_at IS NULL
                  AND pin.expires_at > statement_timestamp()
                  AND pin.operation_expires_at > statement_timestamp()))
           OR EXISTS (
               SELECT 1 FROM runner.turn_checkpoints p
               WHERE p.event_id = h.event_id AND p.ready_at IS NULL
                 AND (p.abandoned_at IS NOT NULL OR p.expires_at <= statement_timestamp())
                 AND NOT EXISTS (
                     SELECT 1 FROM runner.turn_checkpoint_pins pin
                     WHERE pin.turn_checkpoint_id = p.id AND pin.released_at IS NULL
                       AND pin.expires_at > statement_timestamp()
                       AND pin.operation_expires_at > statement_timestamp())),
           false) AS due
FROM page h LEFT JOIN fleet.admissions a ON a.id = h.event_id
ORDER BY h.event_id
