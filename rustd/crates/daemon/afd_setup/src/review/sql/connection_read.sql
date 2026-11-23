-- `connection.sql` without the share lock, for a read's review, which holds nothing.
--
-- $1 workspace · $2 provider · $3 external account · $4 required scopes
WITH held AS MATERIALIZED (
    SELECT id, external_account_id, scopes
      FROM connector.installs
     WHERE workspace_id = $1 AND provider = $2 AND external_account_id = $3
)
SELECT encode(sha256(convert_to(jsonb_agg(
           jsonb_build_array(id, external_account_id, scopes) ORDER BY id
       )::text, 'UTF8')), 'hex') AS version,
       COALESCE(bool_and(scopes @> $4::text[]), false) AS covered
  FROM held
