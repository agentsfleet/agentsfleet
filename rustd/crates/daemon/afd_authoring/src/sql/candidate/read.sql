SELECT id, revision, definition::text AS definition, canonical_digest, policy_semantics
  FROM authoring.candidates
 WHERE id = $1 AND draft_id = $2
