-- Deletes the fleet in whatever status it holds (M249_002 §6), reporting
-- whether a row went.
DELETE FROM fleet.fleets
WHERE id = $1 AND workspace_id = $2
RETURNING id
