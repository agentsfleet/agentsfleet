-- Setup runs the window issued for this run's chat tenants: each is a run its draft owns.
SELECT count(*)
  FROM thread.runs l
  JOIN setup.drafts d ON d.id = l.draft_id
 WHERE d.tenant_id = ANY($1)
