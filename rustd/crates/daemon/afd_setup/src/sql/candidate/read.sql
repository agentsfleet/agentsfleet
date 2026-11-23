-- The draft's own proposal, or its head once that was installed into a fleet.
SELECT r.id, r.revision, r.definition::text AS definition, r.canonical_digest, r.policy_semantics
  FROM fleet.revisions r
  JOIN setup.drafts d ON d.id = $2
 WHERE r.id = $1 AND (r.draft_id = d.id OR d.head_revision_id = r.id)
