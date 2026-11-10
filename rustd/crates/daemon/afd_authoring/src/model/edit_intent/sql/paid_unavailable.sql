SELECT EXISTS (
    SELECT 1 FROM authoring.edit_decisions
    WHERE attempt_id = $1
      AND (accounted_at IS NULL OR charged_nanos > reserved_nanos)
)
