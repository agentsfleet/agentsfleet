-- One approved grant, held under a share lock until the caller commits, so a revoke waits
-- for whatever the caller issues under it.
--
-- $1 fleet · $2 name · $3 the approved status · $4 grant kind
SELECT service FROM connector.integration_grants
WHERE fleet_id = $1 AND service = $2 AND status = $3 AND grant_type = $4
FOR SHARE
