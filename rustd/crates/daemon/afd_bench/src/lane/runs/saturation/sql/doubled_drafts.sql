-- Drafts holding more than one active setup run.
SELECT count(*)
  FROM (SELECT l.draft_id
          FROM thread.runs l
          JOIN setup.drafts d ON d.id = l.draft_id
         WHERE d.tenant_id = ANY($1) AND l.status = $2
         GROUP BY l.draft_id
        HAVING count(*) > 1) doubled
