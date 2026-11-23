SELECT EXISTS (
    SELECT 1 FROM runner.decision_calls
    WHERE fleet_id = $1 AND workspace_id = $2
      AND assessment_key LIKE $3
      AND accounted_at IS NULL
      AND decision_revision_id IS DISTINCT FROM $4
)
