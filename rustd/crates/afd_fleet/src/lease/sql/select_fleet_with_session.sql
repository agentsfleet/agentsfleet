SELECT f.workspace_id::text AS workspace_id, r.definition::text AS definition, f.status,
       r.runtime_model_ref, f.name, s.context_json::text AS context_json, r.policy_semantics
FROM fleet.fleets f
LEFT JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
LEFT JOIN fleet.sessions s ON s.fleet_id = f.id
WHERE f.id = $1::uuid
