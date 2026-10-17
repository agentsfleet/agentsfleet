//! Dispatch identity is durable before a physical request and survives private deletion.

pub(super) const PRIOR: &str = "\
SELECT to_jsonb(d)::text FROM fleet.authoring_dispatches d
WHERE attempt_id=$1::uuid AND (id=$2::uuid OR call_index=$3)";
pub(super) const UNFINISHED: &str = "\
SELECT to_jsonb(d)::text FROM fleet.authoring_dispatches d
WHERE attempt_id=$1::uuid AND status!='completed' ORDER BY call_index LIMIT 1";
pub(super) const INSERT: &str = "\
INSERT INTO fleet.authoring_dispatches(id,attempt_id,call_index,request_digest,status,
output_allowance,private_expires_at,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3,$4,'pending',$5,$6,$7,$7) ON CONFLICT DO NOTHING";
pub(super) const ADVANCE: &str = "\
UPDATE fleet.authoring_attempts SET calls=calls+1,updated_at=$2 WHERE id=$1::uuid";
pub(super) const UNKNOWN: &str = "\
UPDATE fleet.authoring_dispatches SET status='unknown',updated_at=$2 WHERE id=$1::uuid";
pub(super) const UNCERTAIN: &str = "\
UPDATE fleet.authoring_attempts SET usage_unknown=true,status='awaiting_accounting',updated_at=$2 WHERE id=$1::uuid";
pub(super) const LOCK_ATTEMPT: &str = "\
SELECT id::text FROM fleet.authoring_attempts WHERE id=$1::uuid AND runner_id=$2::uuid FOR UPDATE";
pub(super) const COMPLETE: &str = "\
UPDATE fleet.authoring_dispatches SET status=$3,result=$4::jsonb,usage=$5::jsonb,
updated_at=$6,native_reply=$7::jsonb
WHERE id=$1::uuid AND attempt_id=$2::uuid AND status IN ('pending','unknown')";
pub(super) const METER: &str = "\
UPDATE fleet.authoring_attempts SET input_tokens=input_tokens+$2,output_tokens=output_tokens+$3,
status=CASE WHEN $5 THEN 'awaiting_accounting' ELSE status END,
usage_unknown=EXISTS(SELECT 1 FROM fleet.authoring_dispatches
WHERE attempt_id=$1::uuid AND status IN ('pending','unknown')),updated_at=$4 WHERE id=$1::uuid";
