-- What a host's renewal statement needs before it runs (M245 §7), for every
-- lease the runner names at once: each one's pricing facts and payer, whether
-- it runs a durable task, its cancel mark, when it began, the head revision
-- its fleet's ceilings are read from, and the catalogue version prices are
-- read at. Read without a lock; `renew_batch.sql` guards every fact it acts on.
--
-- $1 the leases · $2 the runner
SELECT l.id, l.status, l.cancel_requested_at IS NOT NULL AS cancelled,
       l.tenant_id, l.workspace_id, l.fleet_id, l.posture, l.provider, l.model,
       l.task_id, l.created_at, f.head_revision_id,
       (SELECT c.resource_version FROM model.catalogue_version c WHERE c.id = 1)
           AS catalogue_version
FROM runner.leases l
JOIN fleet.fleets f ON f.id = l.fleet_id
WHERE l.id = ANY($1::uuid[]) AND l.runner_id = $2
