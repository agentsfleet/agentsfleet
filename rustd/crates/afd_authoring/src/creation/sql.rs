//! Fleet lifecycle guards and native revision projection, always workspace-scoped.

/// The fleet row alone. Locked before [`LOCKED`] reads it, never in one join:
/// a joined `FOR UPDATE` that waited re-checks its join against the row a
/// concurrent Apply committed, and the new revision fails that re-check, so a
/// live fleet would read as missing.
pub(super) const LOCK: &str =
    "SELECT id::text FROM core.fleets WHERE id=$1::uuid AND workspace_id=$2::uuid FOR UPDATE";

/// The locked fleet with its current revision, read under a fresh statement
/// snapshot that already includes whichever writer held the lock before us.
pub(super) const LOCKED: &str = "\
SELECT (to_jsonb(f)||jsonb_build_object('definition',r.definition,
'events_processed',coalesce(c.events_processed,0),'budget_used_nanos',coalesce(c.budget_used_nanos,0),
'pending_approvals',(SELECT count(*) FROM core.fleet_approval_gates g WHERE g.fleet_id=f.id AND g.status=$3)))::text
FROM core.fleets f JOIN core.fleet_definition_revisions r ON r.id=f.current_definition_revision_id
LEFT JOIN core.fleet_activity_counters c ON c.fleet_id=f.id
WHERE f.id=$1::uuid AND f.workspace_id=$2::uuid";

pub(super) const CANDIDATE_DRAFT: &str = "\
SELECT d.id::text FROM core.authoring_drafts d JOIN core.authoring_candidates c ON c.draft_id=d.id
WHERE c.id=$1::uuid AND d.workspace_id=$2::uuid AND d.creator_id=$3::uuid";

pub(super) const PUBLISHED: &str = "\
SELECT r.id::text FROM core.fleet_library_revisions r JOIN core.fleet_libraries l ON l.id=r.library_id
WHERE coalesce(l.platform_slug,l.id::text)=$1 AND r.id=$2::uuid AND r.canonical_digest=$3 AND r.policy_semantics=$4
AND (l.tenant_id=$5::uuid OR l.visibility=$6) FOR SHARE OF l";

pub(super) const LIBRARY: &str = "\
SELECT to_jsonb(r)::text FROM core.fleet_library_revisions r
JOIN core.fleet_libraries l ON l.id=r.library_id
WHERE coalesce(l.platform_slug,l.id::text)=$1 AND r.id=$2::uuid AND (l.tenant_id=$3::uuid OR l.visibility=$4)
FOR SHARE OF l";

pub(super) const INSERT: &str = "\
INSERT INTO core.fleets(id,workspace_id,tenant_id,name,config_json,status,
created_at,updated_at,resource_version,setup_version,setup_context,setup_stream_ready,initial_start,setup_runtime_model_ref)
VALUES($1::uuid,$2::uuid,$3::uuid,$4,$5::jsonb,$6,$7,$7,$8,$8,$9::jsonb,false,$10::jsonb,$11)";

pub(super) const REVISION: &str = "\
INSERT INTO core.fleet_definition_revisions(id,fleet_id,revision,definition,canonical_digest,policy_semantics,library_revision_id,created_at,runtime_model_ref)
VALUES($1::uuid,$2::uuid,$3,$4::jsonb,$5,$6,$7::uuid,$8,$9)";

pub(super) const SELECT_REVISION: &str =
    "UPDATE core.fleets SET current_definition_revision_id=$2::uuid WHERE id=$1::uuid";
pub(super) const LINK: &str = "UPDATE core.authoring_drafts SET state=$3,linked_fleet_id=$2::uuid,resource_version=resource_version+1,updated_at=$4 WHERE id=$1::uuid";
pub(super) const STREAM_READY: &str = "UPDATE core.fleets SET setup_stream_ready=true WHERE id=$1::uuid AND workspace_id=$2::uuid AND status=$3";
pub(super) const ACTIVATE: &str = "UPDATE core.fleets SET status=$3,resource_version=resource_version+1,updated_at=$4 WHERE id=$1::uuid AND workspace_id=$2::uuid AND status=$5 AND setup_version=$6 AND setup_stream_ready=true";
pub(super) const INITIAL: &str =
    "UPDATE core.fleets SET initial_start=$3::jsonb WHERE id=$1::uuid AND workspace_id=$2::uuid";
pub(super) const CLOSE_DRAFT: &str = "UPDATE core.authoring_drafts SET state=$2,closed_at=$3,expires_at=$3+$4,resource_version=resource_version+1,updated_at=$3 WHERE linked_fleet_id=$1::uuid AND state=$5";
