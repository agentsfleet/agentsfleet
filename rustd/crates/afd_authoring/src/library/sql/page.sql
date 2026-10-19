SELECT l.id::text AS id,
       l.platform_slug,
       l.name,
       l.description,
       l.credential_reasons::text AS credential_reasons,
       l.updated_at,
       l.visibility,
       l.head_revision_id::text AS head_revision_id,
       l.resource_version,
       l.created_at,
       r.revision AS head_revision,
       r.definition::text AS head_definition,
       r.canonical_digest AS head_canonical_digest,
       r.policy_semantics AS head_policy_semantics,
       r.created_at AS head_created_at
  FROM authoring.library_entries l
  JOIN authoring.library_revisions r ON r.id = l.head_revision_id
 WHERE ((l.tenant_id = $1::uuid AND (NOT $2 OR l.visibility = $7))
        OR (NOT $2 AND l.visibility = $3))
   AND ($4::timestamptz IS NULL OR (l.created_at, l.id) < ($4::timestamptz, $5::uuid))
 ORDER BY l.created_at DESC, l.id DESC
 LIMIT $6
