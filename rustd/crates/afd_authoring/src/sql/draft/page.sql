SELECT id::text AS id,
       title,
       status,
       resource_version,
       fleet_id::text AS fleet_id,
       updated_at,
       expires_at
  FROM authoring.drafts
 WHERE workspace_id = $1::uuid AND user_id = $2::uuid
   AND (expires_at IS NULL OR expires_at > statement_timestamp())
   AND ($3::text IS NULL OR status = $3)
   AND ($4::uuid IS NULL OR fleet_id = $4::uuid)
   AND ($5::timestamptz IS NULL OR (updated_at, id) < ($5::timestamptz, $6::uuid))
 ORDER BY updated_at DESC, id DESC
 LIMIT $7
