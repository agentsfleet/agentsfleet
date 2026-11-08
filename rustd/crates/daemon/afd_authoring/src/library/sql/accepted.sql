SELECT COALESCE(l.platform_slug, l.id::text)
  FROM authoring.library_revisions r
  JOIN authoring.library_entries l ON l.id = r.library_entry_id
 WHERE r.id = $1::uuid
