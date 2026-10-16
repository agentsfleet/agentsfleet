//! Request sequence and its selected model are accepted in the draft transaction.

pub(crate) const REPLAY: &str = "\
SELECT sequence, request_digest FROM core.authoring_messages
WHERE draft_id = $1::uuid AND operation_id = $2::uuid";

pub(crate) const INSERT: &str = "\
INSERT INTO core.authoring_messages
 (id, draft_id, sequence, operation_id, request_digest, authoring_model_ref, state,
  user_text, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4::uuid, $5, $6, $7, $8, $9, $9)";

pub(crate) const ADVANCE: &str = "\
UPDATE core.authoring_drafts SET last_sequence = $2,
resource_version = resource_version + 1, updated_at = $3 WHERE id = $1::uuid";

pub(crate) const CANCEL: &str = "\
UPDATE core.authoring_messages SET state = $3, answer_text = NULL,
finished_at = $4, updated_at = $4
WHERE draft_id = $1::uuid AND sequence <= $2 AND state IN ($5, $6)
RETURNING sequence";

pub(crate) const PAGE: &str = "\
SELECT to_jsonb(m)::text, role.ordinal FROM core.authoring_messages m
CROSS JOIN generate_series(0, 1) AS role(ordinal)
WHERE draft_id = $1::uuid AND (sequence, role.ordinal) > ($2::bigint, $3::integer)
  AND (role.ordinal = 0 OR (state NOT IN ($4, $5) OR answer_text IS NOT NULL))
ORDER BY sequence, role.ordinal LIMIT $6";
