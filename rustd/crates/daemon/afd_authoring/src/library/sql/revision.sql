SELECT id::text AS id, revision, definition::text AS definition, canonical_digest,
       policy_semantics, created_at
  FROM authoring.library_revisions
 WHERE library_entry_id = $1::uuid AND id = $2::uuid
