WITH locked AS MATERIALIZED (
    SELECT id, owner_lease_id, owner_fence, expires_at
    FROM runner.turn_checkpoints
    WHERE event_id = $1 AND ready_at IS NULL AND abandoned_at IS NULL
    ORDER BY generation
    FOR UPDATE
), checked AS MATERIALIZED (
    SELECT id, owner_lease_id, owner_fence, expires_at, clock_timestamp() AS checked_at
    FROM locked
)
UPDATE runner.turn_checkpoints p
SET abandoned_at = c.checked_at, updated_at = now()
FROM checked c
WHERE p.id = c.id
  AND (c.owner_lease_id <> $2 OR c.owner_fence <> $3 OR c.expires_at <= c.checked_at)
