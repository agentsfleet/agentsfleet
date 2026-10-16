//! Immutable versions and the single draft pointer changed under its row lock.

pub(crate) const READ: &str = "SELECT to_jsonb(c)::text FROM core.authoring_candidates c WHERE id = $1::uuid AND draft_id = $2::uuid";

pub(crate) const INSERT: &str = "\
INSERT INTO core.authoring_candidates
(id, draft_id, revision, definition, canonical_digest, policy_semantics, created_at)
VALUES ($1::uuid, $2::uuid, $3, $4::jsonb, $5, $6, $7)";

pub(crate) const SELECT_HEAD: &str = "\
UPDATE core.authoring_drafts SET current_candidate_id = $2::uuid,
candidate_revision = $3, resource_version = resource_version + 1, updated_at = $4
WHERE id = $1::uuid";

pub(crate) const LIBRARY_SOURCE: &str = "\
SELECT r.definition::text FROM core.fleet_library_revisions r
JOIN core.fleet_libraries l ON l.id = r.library_id
WHERE coalesce(l.platform_slug,l.id::text) = $1 AND r.id = $2::uuid AND (l.tenant_id = $3::uuid OR l.visibility = $4)";

pub(crate) const FLEET_SOURCE: &str = "\
SELECT jsonb_build_object('definition',r.definition,'name',f.name,'runtime_model_ref',r.runtime_model_ref)::text FROM core.fleet_definition_revisions r
JOIN core.fleets f ON f.id = r.fleet_id
WHERE f.id = $1::uuid AND f.workspace_id = $2::uuid AND r.revision = $3";
