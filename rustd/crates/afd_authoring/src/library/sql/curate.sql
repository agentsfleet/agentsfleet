UPDATE authoring.library_entries
   SET name = COALESCE($2, name),
       description = COALESCE($3, description),
       credential_reasons = COALESCE($4::jsonb, credential_reasons),
       visibility = COALESCE($5, visibility),
       resource_version = resource_version + 1,
       updated_at = now()
 WHERE id = $1::uuid
   AND (name IS DISTINCT FROM COALESCE($2, name)
        OR description IS DISTINCT FROM COALESCE($3, description)
        OR credential_reasons IS DISTINCT FROM COALESCE($4::jsonb, credential_reasons)
        OR visibility IS DISTINCT FROM COALESCE($5, visibility))
