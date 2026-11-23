SELECT status
  FROM connector.integration_grants
 WHERE fleet_id = $1 AND service = $2
   FOR SHARE
