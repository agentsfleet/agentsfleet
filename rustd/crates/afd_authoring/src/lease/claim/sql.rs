//! Hints never authorize admission; the guarded transaction rereads every fact.

pub(super) const CONTEXT: &str = "admit queued authoring message";
pub(super) const SCAN_LIMIT: i64 = 64;

pub(super) const CANDIDATES: &str = "\
SELECT d.id::text, d.workspace_id::text, d.tenant_id::text, d.creator_id::text
FROM core.authoring_drafts d JOIN core.authoring_messages m ON m.draft_id=d.id
WHERE d.state=$1 AND m.state=$2 AND m.sequence>d.cancelled_through
AND NOT EXISTS (SELECT 1 FROM fleet.authoring_attempts a
 WHERE a.draft_id=d.id AND (a.status<>$3 OR a.usage_unknown))
GROUP BY d.id ORDER BY min(m.created_at),d.id LIMIT $4";

pub(super) const MESSAGE: &str = "\
SELECT sequence,authoring_model_ref FROM core.authoring_messages
WHERE draft_id=$1::uuid AND state=$2 AND sequence>$3 ORDER BY sequence LIMIT 1";

pub(super) const OBLIGATION: &str = "\
SELECT EXISTS(SELECT 1 FROM fleet.authoring_attempts WHERE draft_id=$1::uuid
AND (status<>$2 OR usage_unknown))";

pub(super) const COVERAGE: &str = "\
SELECT w.balance_nanos,count(a.id)::bigint,coalesce(max(a.reserved_nanos),0)::bigint
FROM billing.tenant_wallet w LEFT JOIN fleet.authoring_attempts a
ON a.tenant_id=w.tenant_id AND (a.status IN ('active','awaiting_accounting') OR a.usage_unknown)
WHERE w.tenant_id=$1::uuid GROUP BY w.balance_nanos";

pub(super) const HISTORY: &str = "\
SELECT user_text,answer_text FROM core.authoring_messages
WHERE draft_id=$1::uuid AND sequence<=$2 ORDER BY sequence LIMIT $3";

pub(super) const FAIL: &str = "\
UPDATE core.authoring_messages SET state=$3,failure_reason=$4,finished_at=$5,updated_at=$5
WHERE draft_id=$1::uuid AND sequence=$2 AND state=$6";

pub(super) const INSERT: &str = "\
INSERT INTO fleet.authoring_attempts(id,lease_id,runner_id,workspace_id,tenant_id,creator_id,
draft_id,message_sequence,fence,deletion_generation,capability_hash,model_snapshot,status,
lease_expires_at,deadline,reserved_nanos,settled_nanos,usage_unknown,calls,input_tokens,
output_tokens,activity_sequence,created_at,updated_at,reported_runtime_ms)
VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6::uuid,$7::uuid,$8,$9,$10,$11,$12::jsonb,
$13,$14,$15,$16,0,false,0,0,0,0,$17,$17,0)";

pub(super) const RUNNING: &str = "\
UPDATE core.authoring_messages SET state=$3,updated_at=$4
WHERE draft_id=$1::uuid AND sequence=$2 AND state=$5";
