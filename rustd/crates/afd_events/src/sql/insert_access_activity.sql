INSERT INTO fleet.events
    (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, created_at, updated_at)
SELECT f.id, f.workspace_id, concat($4::text, ':', $2::text, ':', txid_current()),
       $3, $4, $5, $6::jsonb, now(), now()
  FROM fleet.fleets f
  JOIN fleet.revisions r ON r.id = f.head_revision_id
 WHERE f.workspace_id = $1::uuid
   AND ((r.definition #> '{config,x-agentsfleet,credentials}') ? $2
        OR EXISTS (SELECT FROM jsonb_array_elements(COALESCE(
            r.definition #> '{config,x-agentsfleet,triggers}', '[]'::jsonb)) trigger
            WHERE COALESCE(trigger ->> 'credential_name', trigger ->> 'source') = $2))
ON CONFLICT (fleet_id, event_id) DO NOTHING
RETURNING fleet_id::text
