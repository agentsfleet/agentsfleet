SELECT c.fleet_id::text AS fleet_id, f.status
FROM connector.channels c
JOIN fleet.fleets f ON f.id = c.fleet_id
WHERE c.provider = $1 AND c.external_account_id = $2 AND c.external_channel_id = $3
  AND c.binding_type = $4 AND f.workspace_id = $5::uuid
