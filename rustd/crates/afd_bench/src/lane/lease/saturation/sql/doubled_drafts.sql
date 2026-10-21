SELECT count(*)
  FROM (SELECT a.draft_id
          FROM authoring.attempts a
         WHERE a.tenant_id = ANY($1::uuid[]) AND a.status = $2
         GROUP BY a.draft_id
        HAVING count(*) > 1) doubled
