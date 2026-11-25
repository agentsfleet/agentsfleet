-- A fleet's triggers, oldest first.
--
-- $1 fleet
SELECT id, trigger_type, source, events, repositories, verifier_scheme, verifier_header,
       secret_name, channel, created_at
  FROM fleet.triggers
 WHERE fleet_id = $1
 ORDER BY created_at, id
