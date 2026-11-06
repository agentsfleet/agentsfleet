SELECT t.target, t.input_sequence,
       EXISTS (SELECT 1 FROM fleet.task_items i WHERE i.id = $2::uuid AND i.task_id = t.id
               AND i.fleet_id = t.fleet_id AND i.workspace_id = t.workspace_id) AS accepted_item
FROM fleet.tasks t WHERE t.id = $1::uuid
