WITH held AS MATERIALIZED (
    SELECT id, external_account_id, scopes
      FROM connector.installs
     WHERE workspace_id = $1::uuid AND provider = $2 AND external_account_id = $3
       FOR SHARE
)
SELECT encode(sha256(convert_to(jsonb_agg(
           jsonb_build_array(id, external_account_id, scopes) ORDER BY id
       )::text, 'UTF8')), 'hex') AS version,
       COALESCE(bool_and(scopes @> $4::text[]), false) AS covered
  FROM held
