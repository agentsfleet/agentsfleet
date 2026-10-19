INSERT INTO fleet.revisions
    (id, fleet_id, revision, definition, canonical_digest, policy_semantics, runtime_model_ref,
     review_digest, review_context, source_candidate_id, created_at)
VALUES ($1::uuid, $2::uuid, $3, $4::jsonb, $5, $6, $7, $8, $9::jsonb, $10::uuid, now())
