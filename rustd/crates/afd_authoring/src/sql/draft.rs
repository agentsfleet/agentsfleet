//! Private draft rows are always scoped by workspace and creator together.

pub(crate) const LOCK: &str = const_format::concatcp!(
    "\
SELECT to_jsonb(d)::text FROM core.authoring_drafts d
WHERE id = $1::uuid AND workspace_id = $2::uuid AND creator_id = $3::uuid
AND (expires_at IS NULL OR expires_at>",
    afd_db::statement::NOW_MS,
    ")
FOR UPDATE",
);

pub(crate) const CREATE: &str = "\
INSERT INTO core.authoring_drafts
 (id, workspace_id, tenant_id, creator_id, operation_id, creation_digest, title,
  state, authoring_model_ref, candidate_revision, resource_version,
  last_sequence, cancelled_through, deletion_generation, linked_fleet_id, created_at, updated_at, source_pin, instance_name)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, $7, $8, $9,
        $10, $10, $10, $10, $10, $11::uuid, $12, $12, $13::jsonb,$14)";

pub(crate) const ACTIVE_MESSAGES: &str = "\
SELECT EXISTS(SELECT 1 FROM core.authoring_messages
WHERE draft_id = $1::uuid AND state IN ($2, $3))";

pub(crate) const PATCH: &str = "\
UPDATE core.authoring_drafts SET title = $2, authoring_model_ref = $3,
runtime_model_ref = COALESCE($4, runtime_model_ref),
instance_name = COALESCE($6, instance_name),
resource_version = resource_version + 1, updated_at = $5
WHERE id = $1::uuid";

pub(crate) const DELETE: &str = "DELETE FROM core.authoring_drafts WHERE id = $1::uuid";

pub(crate) const CANCEL: &str = "\
UPDATE core.authoring_drafts SET cancelled_through = GREATEST(cancelled_through, $2),
resource_version = resource_version + 1, updated_at = $3 WHERE id = $1::uuid";
