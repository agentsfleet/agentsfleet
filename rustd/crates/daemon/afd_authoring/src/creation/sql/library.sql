SELECT r.id::text AS id, r.revision, r.definition::text AS definition, r.canonical_digest,
       r.policy_semantics
  FROM authoring.library_revisions r
  JOIN authoring.library_entries l ON l.id = r.library_entry_id
 WHERE COALESCE(l.platform_slug, l.id::text) = $1
   AND r.id = $2::uuid
   AND (l.tenant_id = $3::uuid OR l.visibility = $4)
   FOR SHARE OF l
