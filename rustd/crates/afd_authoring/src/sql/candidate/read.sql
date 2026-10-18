SELECT id::text AS id, revision, definition::text AS definition, canonical_digest, policy_semantics
  FROM authoring.candidates
 WHERE id = $1::uuid AND draft_id = $2::uuid
