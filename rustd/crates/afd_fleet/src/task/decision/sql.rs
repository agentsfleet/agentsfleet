//! Decision statements stay with the existing task lifecycle owner.

pub(super) const CANDIDATES: &str = include_str!("sql/candidates.sql");
pub(super) const CLAIM_SOURCE: &str = include_str!("../sql/pending_inputs.sql");
pub(super) const LOCK_TASK: &str = include_str!("sql/lock_task.sql");
pub(super) const PROFILE: &str = include_str!("sql/profile.sql");
pub(super) const PEEK_CALL: &str = include_str!("sql/find_call.sql");
pub(super) const FIND_CALL: &str = const_format::concatcp!(PEEK_CALL, " FOR UPDATE");
pub(super) const TRY_CALL: &str = const_format::concatcp!(FIND_CALL, " NOWAIT");
pub(super) const PAYLOAD: &str = include_str!("sql/payload.sql");
pub(super) const SOURCE_CALLS: &str = include_str!("sql/source_calls.sql");
pub(super) const BUDGET: &str = const_format::concatcp!(
    "WITH charged AS (\n",
    afd_billing::limits::SELECT_FLEET_SPEND,
    "\n), pending AS (\n\
        SELECT COALESCE(SUM(reserved_nanos), 0)::bigint AS nanos\n\
        FROM runner.decision_calls\n\
        WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid AND accounted_at IS NULL\n\
    )\n\
    SELECT (charged.day_nanos + pending.nanos)::bigint AS day_nanos,\n\
           (charged.month_nanos + pending.nanos)::bigint AS month_nanos\n\
    FROM charged CROSS JOIN pending\n",
);
pub(super) const BLOCK_PROFILE_SWITCH: &str = include_str!("sql/block_profile_switch.sql");
pub(super) const SEQUENCE: &str = include_str!("sql/sequence.sql");
pub(super) const INSERT_CALL: &str = include_str!("sql/insert_call.sql");
pub(super) const INSERT_PAYLOAD: &str = include_str!("sql/insert_payload.sql");
pub(super) const RESERVE_TASK: &str = include_str!("sql/reserve_task.sql");
pub(super) const ISSUE: &str = include_str!("sql/issue.sql");
pub(super) const RECORD: &str = include_str!("sql/record.sql");
pub(super) const RECORD_PAYLOAD: &str = include_str!("sql/record_payload.sql");
pub(super) const ACCOUNT_TASK: &str = include_str!("sql/account_task.sql");
pub(super) const LOCK_PARENT: &str = include_str!("sql/lock_parent.sql");
pub(super) const TRY_PARENT: &str = const_format::concatcp!(LOCK_PARENT, " NOWAIT");
pub(super) const TRY_SLOT: &str =
    const_format::concatcp!(crate::task::sql::LOCK_SCOPE_SLOT, " NOWAIT");
pub(super) const TRY_TASK: &str = const_format::concatcp!(LOCK_TASK, " NOWAIT");
pub(super) const SEMANTIC_OWNER: &str = include_str!("sql/semantic_owner.sql");
pub(super) const SEMANTIC_TASK: &str = include_str!("sql/semantic_task.sql");
pub(super) const OVERAGE: &str = include_str!("sql/overage.sql");
pub(super) const EXPIRED_UNISSUED: &str = include_str!("sql/expired_unissued.sql");
