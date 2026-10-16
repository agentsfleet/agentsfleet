//! Content-free replay identity is retained independently from private state.

pub(super) const READ: &str = "\
SELECT actor_id::text, request_digest, resource_id::text, purged_at
FROM core.authoring_replay_receipts WHERE scope = $1 AND operation_id = $2::uuid";

pub(super) const EXPIRE: &str = const_format::concatcp!(
    "\
DELETE FROM core.authoring_replay_receipts WHERE scope = $1 AND operation_id = $2::uuid
AND expires_at <= ",
    afd_db::statement::NOW_MS,
);

pub(super) const INSERT: &str = "\
INSERT INTO core.authoring_replay_receipts
(id, workspace_id, actor_id, scope, operation_id, request_digest, resource_id, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5::uuid, $6, $7::uuid, $8)
ON CONFLICT (scope, operation_id) DO NOTHING";

pub(super) const PURGE: &str = const_format::concatcp!(
    "\
WITH stamp AS MATERIALIZED (
 SELECT ",
    afd_db::statement::NOW_MS,
    " AS now
)
UPDATE core.authoring_replay_receipts SET purged_at = stamp.now, expires_at = stamp.now + $2
FROM stamp WHERE resource_id = $1::uuid AND purged_at IS NULL",
);
