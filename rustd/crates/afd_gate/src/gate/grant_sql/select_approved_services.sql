SELECT service FROM connector.integration_grants
WHERE fleet_id = $1::uuid AND status = $2
