SELECT t.id, t.originating_task_item_id AS originating_item_id, t.target::text,
       t.accepted_revision_id, r.revision AS accepted_revision_number, t.title, t.objective,
       t.questions::text, t.goal_digest, t.status AS state, t.resource_version, t.generation,
       t.stop_generation, t.covered_input_sequence, t.input_sequence, t.task_checkpoint_id,
       c.generation AS checkpoint_generation, t.effect_cursor,
       t.confirmed_effect_watermark, t.max_model_nanos, t.used_model_nanos,
       t.reserved_model_nanos, t.max_running_nanos, t.used_running_nanos,
       t.max_leases, t.used_leases, t.max_publications, t.used_publications,
       t.max_decision_calls, t.used_decision_calls, t.max_fallback_reads,
       t.used_fallback_reads, t.expires_at,
       CASE WHEN w.id IS NULL THEN NULL ELSE jsonb_build_object(
           'active', w.consumed_at IS NULL AND w.cancelled_at IS NULL,
           'head', w.expected_head, 'checks', w.checks, 'updated_at', w.updated_at)::text END AS dependency,
       publication.receipt::text AS publication,
       EXTRACT(EPOCH FROM (t.expires_at - t.created_at))::int4 AS wall_seconds
FROM fleet.tasks t
JOIN fleet.revisions r ON r.id = t.accepted_revision_id AND r.fleet_id = t.fleet_id
LEFT JOIN fleet.task_checkpoints c ON c.id = t.task_checkpoint_id AND c.task_id = t.id
LEFT JOIN LATERAL (
    SELECT id, expected_head, checks, updated_at, consumed_at, cancelled_at FROM fleet.task_waits
    WHERE task_id = t.id AND fleet_id = t.fleet_id AND workspace_id = t.workspace_id
    ORDER BY generation DESC LIMIT 1
) w ON true
LEFT JOIN LATERAL (
    SELECT jsonb_build_object('effect_id', e.id, 'ordinal', e.ordinal, 'state', s.status,
        'expected_head', e.expected_head, 'tested_tree', e.tested_tree, 'commit', s.commit_oid,
        'pull_request_url', t.target->>'url') AS receipt
    FROM fleet.task_effects e JOIN fleet.task_effect_states s ON s.task_effect_id = e.id
    WHERE e.task_id = t.id AND e.fleet_id = t.fleet_id AND e.workspace_id = t.workspace_id
    ORDER BY e.ordinal DESC LIMIT 1
) publication ON true
WHERE t.id = $1 AND t.fleet_id = $2 AND t.workspace_id = $3
