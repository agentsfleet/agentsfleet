SELECT f.workspace_id::text, r.definition::text, f.status,
       r.runtime_model_ref, f.name, s.context_json::text, r.policy_semantics,
       r.id::text AS revision_id, f.head_revision_id::text, t.generation AS task_generation,
       current.definition::text AS current_definition
FROM fleet.fleets f
LEFT JOIN fleet.admissions a ON a.id = $2::uuid AND a.fleet_id = f.id AND a.workspace_id = f.workspace_id
LEFT JOIN fleet.tasks t ON t.id = a.task_id AND t.fleet_id = f.id AND t.workspace_id = f.workspace_id
LEFT JOIN fleet.revisions r ON r.id = COALESCE(t.accepted_revision_id, f.head_revision_id) AND r.fleet_id = f.id
LEFT JOIN fleet.revisions current ON current.id = f.head_revision_id AND current.fleet_id = f.id
LEFT JOIN fleet.sessions s ON s.fleet_id = f.id
WHERE f.id = $1::uuid
