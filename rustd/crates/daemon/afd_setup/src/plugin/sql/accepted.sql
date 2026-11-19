SELECT l.id
  FROM authoring.plugin_revisions r
  JOIN authoring.plugin_entries l ON l.id = r.plugin_entry_id
 WHERE r.id = $1
