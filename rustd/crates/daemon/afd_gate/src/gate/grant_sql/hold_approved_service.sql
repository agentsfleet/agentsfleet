SELECT service FROM connector.integration_grants
WHERE fleet_id = $1 AND service = $2 AND status = $3
FOR SHARE
