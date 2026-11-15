SELECT id, revision, definition::text AS definition, canonical_digest,
       policy_semantics, created_at
  FROM authoring.plugin_revisions
 WHERE plugin_entry_id = $1 AND id = $2
