SELECT EXISTS (
    SELECT 1 FROM runner.decision_calls c
    WHERE c.fleet_id = $4 AND c.workspace_id = $5
      AND c.charged_nanos > c.reserved_nanos
      AND (
          c.original_lease_id = $1 OR c.source_admission_id = $2
          OR c.task_id = $3 OR c.source_admission_id IN (
              SELECT i.admission_id FROM fleet.task_items i
              WHERE i.task_id = $3 AND i.fleet_id = c.fleet_id
                AND i.workspace_id = c.workspace_id
          )
      )
)
