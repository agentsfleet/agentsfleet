WITH held AS MATERIALIZED (
    SELECT id, created_at FROM fleet.durable_task_identity_keys
    WHERE id=$1 AND workspace_id=$2 AND repository=$3 FOR UPDATE
), fresh AS MATERIALIZED (
    SELECT clock_timestamp() AS at FROM held
)
DELETE FROM fleet.durable_task_identity_keys k USING held h, fresh f
WHERE k.id=h.id AND h.created_at<f.at-make_interval(secs=>$4)
  AND NOT EXISTS (SELECT 1 FROM fleet.durable_task_objects o WHERE o.workspace_id=$2 AND o.durable_task_identity_key_id=k.id)
  AND NOT EXISTS (SELECT 1 FROM fleet.durable_task_storage_purges p
      WHERE p.durable_task_identity_key_id=k.id AND p.completed_at IS NULL)
