INSERT INTO fleet.revisions
    (id, draft_id, revision, definition, canonical_digest, policy_semantics, installed,
     created_at, updated_at)
VALUES ($1, $2, $3, $4::jsonb, $5, $6, false, now(), now())
