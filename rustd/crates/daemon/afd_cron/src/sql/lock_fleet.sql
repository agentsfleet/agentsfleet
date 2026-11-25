-- Takes the fleet's turn among its schedule writers until the transaction ends: an advisory
-- lock, so intake, reports and items of the fleet never wait on a create. The row is held KEY
-- SHARE first, the mode the insert's foreign key takes anyway, so a writer that holds the fleet
-- row exclusively (a purge deleting it) is waited out before the turn is taken.
WITH fleet AS MATERIALIZED (
    SELECT f.id
      FROM fleet.fleets f
     WHERE f.id = $1
       FOR KEY SHARE OF f
)
SELECT fleet.id
  FROM fleet
 CROSS JOIN LATERAL (SELECT pg_advisory_xact_lock(uuid_hash(fleet.id))) AS writers
