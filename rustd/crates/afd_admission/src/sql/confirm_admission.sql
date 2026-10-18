SELECT id::text AS id, fleet_id::text AS fleet_id, workspace_id::text AS workspace_id,
       actor, event_type, request_json, event_created_at, receipt
FROM fleet.admissions
WHERE producer = $1 AND producer_key = $2
FOR UPDATE SKIP LOCKED
