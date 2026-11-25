-- Whether a fleet being purged still holds a schedule. Read under the purge's row lock, which a
-- schedule insert's KEY SHARE waits on, so no schedule lands after the answer.
SELECT EXISTS (
    SELECT 1
      FROM fleet.schedules s
      JOIN fleet.fleets f ON f.id = s.fleet_id
     WHERE s.fleet_id = $1 AND f.workspace_id = $2
)
