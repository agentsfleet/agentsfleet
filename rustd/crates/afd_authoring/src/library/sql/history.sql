SELECT id::text AS id, revision, canonical_digest, created_at
  FROM authoring.library_revisions
 WHERE library_entry_id = $1::uuid
   AND ($2::timestamptz IS NULL OR (created_at, id) < ($2::timestamptz, $3::uuid))
 ORDER BY created_at DESC, id DESC
 LIMIT $4
