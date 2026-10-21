WITH slot AS MATERIALIZED (
  SELECT s.fleet_id FROM runner.fleet_slots s
  WHERE s.fleet_id = $1::uuid AND s.fencing_token = $3
  FOR UPDATE OF s
), admission AS (
  SELECT a.id FROM fleet.admissions a JOIN slot s ON s.fleet_id = a.fleet_id
  WHERE a.id = $2::uuid AND a.lease_fence = $3 AND a.settled_at IS NULL
  FOR UPDATE OF a
)
SELECT EXISTS (SELECT 1 FROM admission)
