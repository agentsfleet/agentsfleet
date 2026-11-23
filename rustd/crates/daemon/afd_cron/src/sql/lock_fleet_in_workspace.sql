-- `lock_fleet.sql`'s turn, for a fleet in the caller's workspace; no row takes no turn.
WITH fleet AS MATERIALIZED (
    SELECT f.id
      FROM fleet.fleets f
     WHERE f.id = $1 AND f.workspace_id = $2
       FOR KEY SHARE OF f
)
SELECT fleet.id
  FROM fleet
 CROSS JOIN LATERAL (SELECT pg_advisory_xact_lock(uuid_hash(fleet.id))) AS writers
