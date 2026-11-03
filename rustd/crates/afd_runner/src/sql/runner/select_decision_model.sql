-- Deferred identity/owner constraints and activation prove an owned reviewed row.
-- Reading only the locked tuple remains current across a concurrent revision change.
SELECT m.decision_enabled
FROM model.platform_models m
WHERE m.id = $1::uuid
FOR SHARE OF m
