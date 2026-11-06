-- An obsolete native continuation leaves the fleet's backlog count when its
-- generation moves, issued or not. An issued one's late report is then refused
-- by the fence guard and bills nothing further (Indy, M242_001 Discovery).
UPDATE fleet.admissions a SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
FROM fleet.tasks t WHERE t.id = $1::uuid AND a.task_id = t.id AND a.fleet_id = t.fleet_id
  AND a.workspace_id = t.workspace_id AND a.settled_at IS NULL AND a.producer = $2
  AND a.task_generation IS DISTINCT FROM t.generation
