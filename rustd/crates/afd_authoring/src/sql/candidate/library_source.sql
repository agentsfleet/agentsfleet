SELECT r.definition::text
  FROM authoring.library_revisions r
  JOIN authoring.library_entries l ON l.id = r.library_entry_id
 WHERE COALESCE(l.platform_slug, l.id::text) = $1
   AND r.id = $2::uuid
   AND (l.tenant_id = $3::uuid OR l.visibility = $4)
