SELECT l.platform_slug
  FROM authoring.library_entries l
  JOIN authoring.library_revisions r ON r.library_entry_id = l.id
 WHERE r.id = $1::uuid AND l.platform_slug IS NOT NULL
