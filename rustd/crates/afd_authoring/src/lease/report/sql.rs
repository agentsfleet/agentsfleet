//! Content-free acknowledgement and final money writes are protected by the attempt lock.

pub(super) const RECEIPT: &str = "\
SELECT accounting_digest,acknowledgement::text FROM billing.authoring_report_receipts
WHERE attempt_id=$1::uuid AND report_id=$2::uuid";
pub(super) const ANSWER: &str = const_format::concatcp!(
    "\
SELECT answer_digest FROM core.authoring_answer_receipts
WHERE attempt_id=$1::uuid AND report_id=$2::uuid
AND expires_at>",
    afd_db::statement::NOW_MS,
);
pub(super) const ACKNOWLEDGE: &str = "\
INSERT INTO billing.authoring_report_receipts(id,attempt_id,report_id,accounting_digest,acknowledgement,created_at)
VALUES($1::uuid,$2::uuid,$3::uuid,$4,$5::jsonb,$6)";
pub(super) const ANSWER_RECEIPT: &str = "\
INSERT INTO core.authoring_answer_receipts(id,draft_id,attempt_id,report_id,answer_digest,expires_at,created_at)
VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5,$6,$7)";
pub(super) const DEBIT: &str = "\
WITH charge AS (SELECT LEAST(balance_nanos,$2::bigint) AS amount FROM billing.tenant_wallet WHERE tenant_id=$1::uuid)
UPDATE billing.tenant_wallet w SET balance_nanos=w.balance_nanos-c.amount,updated_at=$3
FROM charge c WHERE w.tenant_id=$1::uuid RETURNING c.amount";
pub(super) const SETTLEMENT: &str = "\
INSERT INTO billing.authoring_settlements(id,attempt_id,report_id,accounting_digest,outcome,failure_reason,
usage,charged_nanos,created_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4,$5,$6,$7::jsonb,$8,$9)";
pub(super) const ATTEMPT: &str = "\
UPDATE fleet.authoring_attempts SET status=$2,settled_nanos=$3,usage_unknown=$4,
reported_runtime_ms=$5,lease_expires_at=LEAST(lease_expires_at,$6),updated_at=$6 WHERE id=$1::uuid";
/// The message's first terminal write: it matches only a running message, so
/// the one report that finishes it is the one that matches.
pub(super) const MESSAGE: &str = const_format::concatcp!(MESSAGE_SET, " AND state='running'");
/// A late report rewriting a message expiry already failed for unknown usage;
/// the message finished then, so this write is not a second finish.
pub(super) const LATE_MESSAGE: &str = const_format::concatcp!(
    MESSAGE_SET,
    " AND state<>'running' AND failure_reason='provider_usage_unknown'"
);
const MESSAGE_SET: &str = "\
UPDATE core.authoring_messages SET state=$3,answer_text=$4,candidate_id=$5::uuid,usage=$6::jsonb,
failure_reason=$7,finished_at=$8,updated_at=$8 WHERE draft_id=$1::uuid AND sequence=$2";
pub(super) const RESOLVED_USAGE: &str = "\
UPDATE core.authoring_messages SET usage=$3::jsonb,failure_reason=$4,updated_at=$5
WHERE draft_id=$1::uuid AND sequence=$2 AND failure_reason=$6";
pub(super) const SETTLED_OUTCOME: &str = "\
SELECT to_jsonb(outcome)::text FROM billing.authoring_settlements WHERE attempt_id=$1::uuid";
