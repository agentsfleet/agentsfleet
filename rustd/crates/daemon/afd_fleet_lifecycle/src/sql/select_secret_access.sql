-- Which runnable fleets in a workspace may read each secret, by name.
-- A run resolves exactly the names its revision declares. A task's accepted
-- revision installs only when its runtime authority, credentials included,
-- equals the head's (`Runs::install`, `same_task_authority`), so the head's
-- declared names are the whole answer: a task revision that differs cannot run.
-- Trigger verification and textual references grant nothing here.
WITH governing AS (
    SELECT f.id, f.name, f.head_revision_id AS revision_id
      FROM fleet.fleets f
     WHERE f.workspace_id = $1 AND f.status = $2
), declared AS (
    SELECT g.id, g.name, r.definition #> '{config,x-agentsfleet,credentials}' AS names
      FROM governing g
      JOIN fleet.revisions r ON r.id = g.revision_id AND r.fleet_id = g.id
)
SELECT DISTINCT credential.name AS secret_name, d.id AS fleet_id, d.name AS fleet_name
  FROM declared d
 CROSS JOIN LATERAL jsonb_array_elements_text(
       CASE WHEN jsonb_typeof(d.names) = 'array' THEN d.names ELSE '[]'::jsonb END
   ) AS credential(name)
 WHERE credential.name IS NOT NULL
 ORDER BY secret_name, fleet_name, fleet_id
