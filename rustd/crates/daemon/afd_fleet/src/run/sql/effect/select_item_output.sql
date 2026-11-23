-- The answer a call's effect gave, once its item is written. Walks `uq_items_run_id_sequence`.
--
-- $1 run · $2 the call's number in it
SELECT output
  FROM thread.items
 WHERE run_id = $1 AND sequence = $2
