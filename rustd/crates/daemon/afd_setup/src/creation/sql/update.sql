-- One change to a locked fleet's row, under the tag its caller checked.
--
-- $1 fleet · $2 workspace · $3 status · $4 name · $5 model · $6 HTTP egress · $7 package domains
UPDATE fleet.fleets
   SET status = $3, name = $4, model_ref = $5, http_egress = $6, package_domains = $7,
       resource_version = resource_version + 1, updated_at = now()
 WHERE id = $1 AND workspace_id = $2
