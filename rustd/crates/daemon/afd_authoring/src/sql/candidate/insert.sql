INSERT INTO authoring.candidates
    (id, draft_id, revision, definition, canonical_digest, policy_semantics, created_at)
VALUES ($1, $2, $3, $4::jsonb, $5, $6, now())
