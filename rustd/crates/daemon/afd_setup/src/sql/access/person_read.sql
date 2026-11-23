-- `person.sql` without `identity.lock_user`, for a read or an assignment that holds nothing:
-- a person deleted while it runs is no worse than one deleted a moment after it answers.
--
-- $1 workspace · $2 authenticated identity-provider subject
SELECT u.id AS user_id,
       w.tenant_id AS owner_id,
       u.tenant_id = w.tenant_id AS personal_owner
  FROM identity.users u
  JOIN identity.workspaces w ON w.id = $1
 WHERE u.oidc_subject = $2
