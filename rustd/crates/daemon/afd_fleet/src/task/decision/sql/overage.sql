SELECT EXISTS (
    SELECT 1 FROM runner.decision_calls c
    WHERE c.fleet_id = $4::uuid AND c.workspace_id = $5::uuid
      AND c.charged_nanos > c.reserved_nanos
      AND (
          c.original_lease_id = $1::uuid OR c.source_admission_id = $2::uuid
          OR ($3::uuid IS NOT NULL AND (
              c.task_id = $3::uuid OR c.source_admission_id IN (
                  SELECT i.admission_id FROM fleet.task_items i
                  WHERE i.task_id = $3::uuid AND i.fleet_id = c.fleet_id
                    AND i.workspace_id = c.workspace_id
              )
          ))
      )
)
