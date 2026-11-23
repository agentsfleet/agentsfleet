INSERT INTO fleet.revisions
    (id, fleet_id, revision, definition, canonical_digest, policy_semantics, installed,
     runtime_model_ref, review_digest, review_context, created_at, updated_at)
VALUES ($1, $2, $3, $4::jsonb, $5, $6, true, $7, $8, $9::jsonb, now(), now())
