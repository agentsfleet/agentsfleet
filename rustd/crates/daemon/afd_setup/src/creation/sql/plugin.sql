SELECT r.id AS id, r.revision, r.definition::text AS definition, r.canonical_digest,
       r.policy_semantics
  FROM setup.plugin_revisions r
  JOIN setup.plugin_entries l ON l.id = r.plugin_entry_id
 WHERE l.id = $1
   AND r.id = $2
   AND l.tenant_id = $3
   FOR SHARE OF l
