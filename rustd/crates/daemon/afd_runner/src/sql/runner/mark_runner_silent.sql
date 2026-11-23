-- The liveness visit's compare-and-set, and its row lock: the row matches only while
-- the beat read when the connection ended is still the runner's last. An `active`
-- runner is cordoned for missed heartbeats; any other status keeps its cordon fields.
-- $1 runner · $2 the observed beat · $3 the never-seen sentinel · $4 active · $5 reason
UPDATE runner.runners
SET cordon_reason = CASE WHEN status = $4 THEN $5 ELSE cordon_reason END,
    cordoned_at = CASE WHEN status = $4 THEN COALESCE(cordoned_at, now()) ELSE cordoned_at END,
    cordon_healthy_since_at = CASE WHEN status = $4 THEN NULL ELSE cordon_healthy_since_at END,
    updated_at = now()
WHERE id = $1 AND last_seen_at = $2 AND last_seen_at <> $3
RETURNING old.cordon_reason IS NULL AND new.cordon_reason IS NOT NULL AS cordoned
