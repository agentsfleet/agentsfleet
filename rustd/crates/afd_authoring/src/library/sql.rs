//! Native Library rows are scoped by the authenticated workspace owner.

pub(super) const ENTRY: &str = "SELECT to_jsonb(l)::text FROM core.fleet_libraries l WHERE coalesce(platform_slug,id::text)=$1 AND (tenant_id=$2::uuid OR visibility=$3) FOR SHARE";
pub(super) const OWNED: &str = "SELECT to_jsonb(l)::text FROM core.fleet_libraries l WHERE coalesce(platform_slug,id::text)=$1 AND tenant_id=$2::uuid AND visibility=$3 FOR UPDATE";
pub(super) const REVISION: &str = "SELECT to_jsonb(r)::text FROM core.fleet_library_revisions r WHERE library_id=$1::uuid AND id=$2::uuid";
pub(super) const SOURCE: &str = "SELECT d.id::text FROM core.authoring_drafts d JOIN core.authoring_candidates c ON c.draft_id=d.id WHERE c.id=$1::uuid AND d.workspace_id=$2::uuid AND d.creator_id=$3::uuid";
pub(super) const INSERT: &str = "INSERT INTO core.fleet_libraries(id,tenant_id,visibility,name,description,head_revision_id,resource_version,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3,$4,$5,$6::uuid,$7,$8,$8)";
pub(super) const APPEND: &str = "INSERT INTO core.fleet_library_revisions(id,library_id,revision,definition,canonical_digest,policy_semantics,created_at) VALUES($1::uuid,$2::uuid,$3,$4::jsonb,$5,$6,$7)";
pub(super) const ADVANCE: &str = "UPDATE core.fleet_libraries SET head_revision_id=$2::uuid,resource_version=resource_version+1,updated_at=$3 WHERE id=$1::uuid";
pub(super) const RECEIPT: &str = "UPDATE core.authoring_drafts SET publication=$2::jsonb,resource_version=resource_version+1,updated_at=$3 WHERE id=$1::uuid";
