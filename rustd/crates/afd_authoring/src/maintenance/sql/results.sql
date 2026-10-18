WITH expired AS (
    SELECT id FROM authoring.dispatches
     WHERE private_expires_at <= statement_timestamp() AND result IS NOT NULL
     ORDER BY private_expires_at, id
     LIMIT $1
       FOR UPDATE SKIP LOCKED
)
UPDATE authoring.dispatches d
   SET result = NULL, updated_at = now()
  FROM expired
 WHERE d.id = expired.id
