-- The setup documents of revisions the definitions cache does not hold, so a
-- host's renewal reads its fleets' ceilings with one read for every miss.
--
-- $1 the revisions
SELECT r.id, r.definition::text AS definition
FROM fleet.revisions r
WHERE r.id = ANY($1::uuid[])
