SELECT r.request_digest, r.candidate_id AS candidate_id, c.revision AS candidate_revision
  FROM setup.activity_receipts r
  LEFT JOIN setup.candidates c ON c.id = r.candidate_id
 WHERE r.attempt_id = $1 AND r.sequence = $2
