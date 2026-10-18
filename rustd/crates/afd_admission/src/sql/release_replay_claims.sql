UPDATE fleet.admissions
SET replay_claim_expires_at = NULL, updated_at = now()
WHERE id = ANY($1::text[]::uuid[]) AND replay_claim_expires_at IS NOT NULL
