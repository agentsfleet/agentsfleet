-- One access receipt on each fleet of the workspace that holds a grant for the
-- stored name or a trigger it signs, so the fleet's history shows the access
-- it asked for, or is woken through, arriving.
--
-- $1 workspace · $2 stored name · $3 actor · $4 type · $5 status · $6 metadata
INSERT INTO thread.turns
    (id, thread_id, workspace_id, actor, kind, status, request_json,
     opened_at, settled_at, created_at, updated_at)
SELECT uuidv7(), f.id, f.workspace_id,
       $3, $4, $5, $6::jsonb, now(), now(), now(), now()
  FROM fleet.fleets f
 WHERE f.workspace_id = $1
   AND (EXISTS (SELECT FROM connector.integration_grants g
                 WHERE g.fleet_id = f.id AND g.service = $2)
        OR EXISTS (SELECT FROM fleet.triggers t
                    WHERE t.fleet_id = f.id AND t.workspace_id = $1
                      AND COALESCE(t.secret_name, t.source) = $2))
ON CONFLICT (id) DO NOTHING
RETURNING thread_id
