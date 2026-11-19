SELECT r.definition::text
  FROM setup.plugin_revisions r
  JOIN setup.plugin_entries l ON l.id = r.plugin_entry_id
 WHERE l.id = $1
   AND r.id = $2
   AND l.tenant_id = $3
