UPDATE fleet.admissions a
SET receipt = r.receipt, replay_count = a.replay_count + 1,
    replay_claim_expires_at = NULL, updated_at = now()
FROM unnest($1::text[]::uuid[], $2::text[]) AS r(id, receipt)
WHERE a.id = r.id AND a.receipt IS NULL
RETURNING a.id::text AS id, a.replay_count
