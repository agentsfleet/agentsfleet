-- The digest of the receipt this item and mode already hold, then of any receipt the operation
-- already made in the fleet. An apply lives on the item's input ($6 names its mode); a reply
-- lives on the item.
SELECT n.request_digest FROM fleet.durable_task_inputs n
WHERE n.durable_task_item_id = $1 AND $2 = $6 AND n.fleet_id = $3 AND n.workspace_id = $4
  AND n.request_digest IS NOT NULL
UNION ALL
SELECT i.request_digest FROM fleet.durable_task_items i
WHERE i.id = $1 AND i.mode = $2 AND i.fleet_id = $3 AND i.workspace_id = $4
UNION ALL
SELECT n.request_digest FROM fleet.durable_task_inputs n
WHERE n.operation_id = $5 AND n.fleet_id = $3 AND n.workspace_id = $4
  AND NOT (n.durable_task_item_id = $1 AND $2 = $6)
UNION ALL
SELECT i.request_digest FROM fleet.durable_task_items i
WHERE i.operation_id = $5 AND i.fleet_id = $3 AND i.workspace_id = $4
  AND NOT (i.id = $1 AND i.mode = $2)
