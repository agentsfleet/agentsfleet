-- `grant.sql` without the share lock, for a read's review, which holds nothing.
--
-- $1 fleet · $2 service
SELECT status
  FROM connector.integration_grants
 WHERE fleet_id = $1 AND service = $2
