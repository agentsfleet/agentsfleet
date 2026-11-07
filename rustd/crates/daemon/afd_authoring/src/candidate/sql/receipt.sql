SELECT r.request_digest, r.candidate_id::text AS candidate_id, c.revision AS candidate_revision
  FROM authoring.activity_receipts r
  LEFT JOIN authoring.candidates c ON c.id = r.candidate_id
 WHERE r.attempt_id = $1::uuid AND r.sequence = $2
