SELECT l.id AS id,
       l.name,
       l.description,
       l.credential_reasons::text AS credential_reasons,
       l.head_revision_id AS head_revision_id,
       l.resource_version,
       l.created_at,
       r.revision AS head_revision,
       r.definition::text AS head_definition,
       r.canonical_digest AS head_canonical_digest,
       r.policy_semantics AS head_policy_semantics,
       r.created_at AS head_created_at
  FROM setup.plugin_entries l
  JOIN setup.plugin_revisions r ON r.id = l.head_revision_id
 WHERE l.tenant_id = $1
   AND ($2::timestamptz IS NULL OR (l.created_at, l.id) < ($2::timestamptz, $3))
 ORDER BY l.created_at DESC, l.id DESC
 LIMIT $4
