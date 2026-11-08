INSERT INTO fleet.tasks
  (id, fleet_id, workspace_id, tenant_id, originating_task_item_id, accepted_revision_id,
   title, objective, questions, goal_digest, target, status, resource_version,
   generation, stop_generation, input_sequence, covered_input_sequence,
   effect_cursor, confirmed_effect_watermark, max_model_nanos, used_model_nanos,
   reserved_model_nanos, max_running_nanos, used_running_nanos, max_leases,
   used_leases, max_publications, used_publications, max_decision_calls,
   used_decision_calls, max_fallback_reads, used_fallback_reads,
   expires_at, terminal_at, created_at, updated_at)
SELECT $1, f.id, f.workspace_id, f.tenant_id, $3, f.head_revision_id,
       $4, 'Repair the accepted redirect defect.', $5::jsonb, repeat('b', 64),
       '{"kind":"api_only"}'::jsonb, $6, 1, 1, 0, 0, 0, 0, 0,
       5000000000, 12000000, 20000000, 7200000000000, 1000000000,
       12, 1, 8, 0, 128, 1, 24, 0,
       now() + interval '7 days' - $8::bigint * interval '1 millisecond',
       CASE WHEN $7 THEN now() ELSE NULL END,
       now() - $8::bigint * interval '1 millisecond', now()
FROM fleet.fleets f WHERE f.id = $2
