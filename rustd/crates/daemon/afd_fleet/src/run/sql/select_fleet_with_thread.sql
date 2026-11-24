-- One fleet's row and its session checkpoint, as a run runs them. LEFT JOIN: a fleet that has
-- never checkpointed has no resume context, and its first run starts fresh.
--
-- $1 fleet
SELECT f.workspace_id, f.status, f.name, f.model_ref, f.http_egress, f.package_domains,
       s.resume_context::text AS context_json
FROM fleet.fleets f
LEFT JOIN thread.threads s ON s.id = f.id
WHERE f.id = $1
