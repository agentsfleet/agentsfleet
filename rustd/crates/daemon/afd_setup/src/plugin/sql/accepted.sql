SELECT l.id
  FROM setup.plugin_revisions r
  JOIN setup.plugin_entries l ON l.id = r.plugin_entry_id
 WHERE r.id = $1
