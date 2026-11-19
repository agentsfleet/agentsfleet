WITH expired AS (
    SELECT id FROM setup.dispatches
     WHERE private_expires_at <= statement_timestamp()
       AND (result IS NOT NULL OR native_reply IS NOT NULL)
     ORDER BY private_expires_at, id
     LIMIT $1
       FOR UPDATE SKIP LOCKED
)
UPDATE setup.dispatches d
   SET result = NULL, native_reply = NULL, updated_at = now()
  FROM expired
 WHERE d.id = expired.id
