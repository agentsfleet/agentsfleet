-- One coarse recovery check per replica and tick, never one query per idle worker.
-- EXISTS stops after a match; claim transactions still decide tags, policy, gates and capacity.
-- A false positive costs a bounded claim pass. A negative must not hide pending work.
SELECT EXISTS (
    SELECT 1 FROM fleet.admissions a
    WHERE a.settled_at IS NULL
      AND ($4::boolean OR (
        (a.task_id IS NOT NULL OR NOT EXISTS (
          SELECT 1 FROM runner.turn_recovery recovery
          WHERE recovery.event_id = a.id AND recovery.head_turn_checkpoint_id IS NOT NULL
        ))
        AND NOT EXISTS (
          SELECT 1 FROM fleet.admissions earlier
          WHERE earlier.fleet_id = a.fleet_id AND earlier.settled_at IS NULL
            AND earlier.sequence < a.sequence
        )
      ))
      AND NOT EXISTS (
          SELECT 1 FROM runner.fleet_slots s
          WHERE s.fleet_id = a.fleet_id AND s.lease_expires_at > statement_timestamp()
      )
) OR EXISTS (
    SELECT 1 FROM authoring.drafts d
    JOIN authoring.messages m ON m.draft_id = d.id
    WHERE d.status = $2 AND m.status = $3 AND m.sequence > d.cancelled_through_sequence
      AND NOT EXISTS (
          SELECT 1 FROM authoring.attempts a
          WHERE a.draft_id = d.id AND a.status = $1
            AND a.lease_expires_at > statement_timestamp() AND a.expires_at > statement_timestamp()
      )
)
