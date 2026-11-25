-- What a host's renewal statement needs before it runs, for every
-- run the runner names at once: each one's pricing facts and payer, whether
-- it runs a durable task, its cancel mark, when it began, and the catalogue
-- version prices are read at. Read without a lock; `renew_batch.sql` guards
-- every fact it acts on.
--
-- $1 the runs · $2 the runner
SELECT l.id, l.status, l.cancel_requested_at IS NOT NULL AS cancelled,
       l.tenant_id, l.workspace_id, l.fleet_id, l.posture, l.provider, l.model,
       l.durable_task_id, l.created_at,
       (SELECT c.number FROM model.revision c WHERE c.id = 1)
           AS catalogue_version
FROM thread.runs l
WHERE l.id = ANY($1::uuid[]) AND l.runner_id = $2
