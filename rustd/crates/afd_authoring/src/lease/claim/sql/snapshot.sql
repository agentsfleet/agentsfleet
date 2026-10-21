SELECT resource_version,
       head_candidate_id::text AS head_candidate_id,
       cancelled_through_sequence
  FROM authoring.drafts
 WHERE id = $1::uuid AND status = $2
