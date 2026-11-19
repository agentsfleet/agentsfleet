INSERT INTO thread.turns
    (id, thread_id, workspace_id, actor, kind, status, request_json,
     opened_at, settled_at, created_at, updated_at)
SELECT uuidv7(), f.id, f.workspace_id,
       $3, $4, $5, $6::jsonb, now(), now(), now(), now()
  FROM fleet.fleets f
  JOIN fleet.revisions r ON r.id = f.head_revision_id
 WHERE f.workspace_id = $1
   AND ((r.definition #> '{config,x-agentsfleet,credentials}') ? $2
        OR EXISTS (SELECT FROM jsonb_array_elements(COALESCE(
            r.definition #> '{config,x-agentsfleet,triggers}', '[]'::jsonb)) trigger
            WHERE COALESCE(trigger ->> 'credential_name', trigger ->> 'source') = $2))
ON CONFLICT (id) DO NOTHING
RETURNING thread_id
