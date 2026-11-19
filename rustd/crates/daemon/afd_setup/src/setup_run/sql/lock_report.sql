-- The draft a draft run's report finishes, locked before the run and its slot, and the
-- message the run ran. No row once the draft is deleted.
--
-- $1 draft · $2 the message the run ran
SELECT d.workspace_id, d.tenant_id, d.user_id, m.sequence, m.status, m.save_requested,
       u.oidc_subject
  FROM setup.drafts d
  JOIN setup.messages m ON m.draft_id = d.id AND m.id = $2
  JOIN identity.users u ON u.id = d.user_id
 WHERE d.id = $1
   FOR UPDATE OF d
