SELECT status
  FROM connector.integration_grants
 WHERE fleet_id = $1::uuid AND service = $2
   FOR SHARE
