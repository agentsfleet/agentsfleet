-- Deletes the fleet in whatever status it holds, since delete and archive are
-- separate operations; reports whether a row went.
DELETE FROM fleet.fleets
WHERE id = $1 AND workspace_id = $2
RETURNING id
