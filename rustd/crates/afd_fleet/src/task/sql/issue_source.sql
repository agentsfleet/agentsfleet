SELECT a.task_id::text, a.task_generation AS admission_generation, COALESCE(t.accepted_revision_id, f.head_revision_id)::text AS accepted_revision_id,
       t.generation, f.head_revision_id::text,
       (r.durable_tasks_enabled AND COALESCE(r.capability_report @> jsonb_build_object($4::text, true), false)) AS capable,
       COALESCE(setup.definition #> $5::text[] IS NOT NULL AND setup.definition #> $5::text[] <> 'null'::jsonb, false) AS requires_tasks
FROM fleet.admissions a
JOIN fleet.fleets f ON f.id = a.fleet_id AND f.workspace_id = a.workspace_id
JOIN runner.runners r ON r.id = $3::uuid
LEFT JOIN fleet.revisions setup ON setup.id = f.head_revision_id AND setup.fleet_id = f.id
LEFT JOIN fleet.tasks t ON t.id = a.task_id AND t.fleet_id = a.fleet_id AND t.workspace_id = a.workspace_id
WHERE a.id = $1::uuid AND a.fleet_id = $2::uuid AND a.settled_at IS NULL
