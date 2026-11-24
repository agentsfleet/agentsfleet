//! Decision statements stay with the existing task lifecycle owner.

pub(super) const LOCK_DURABLE_TASK: &str = include_str!("sql/lock_durable_task.sql");
pub(super) const PROFILE: &str = include_str!("sql/profile.sql");
pub(super) const PEEK_CALL: &str = include_str!("sql/find_call.sql");
pub(super) const FIND_CALL: &str = const_format::concatcp!(PEEK_CALL, " FOR UPDATE");
pub(super) const PAYLOAD: &str = include_str!("sql/payload.sql");
pub(super) const BLOCK_PROFILE_SWITCH: &str = include_str!("sql/block_profile_switch.sql");
pub(super) const CONTEXT: &str = include_str!("sql/context.sql");
pub(super) const SOURCE: &str = include_str!("sql/source.sql");
pub(super) const ISSUE: &str = include_str!("sql/issue.sql");
pub(super) const RECORD: &str = include_str!("sql/record.sql");
pub(super) const RECORD_PAYLOAD: &str = include_str!("sql/record_payload.sql");
pub(super) const ACCOUNT_DURABLE_TASK: &str = include_str!("sql/account_durable_task.sql");
pub(super) const LOCK_PARENT: &str = include_str!("sql/lock_parent.sql");
pub(super) const SEMANTIC_OWNER: &str = include_str!("sql/semantic_owner.sql");
pub(super) const SEMANTIC_DURABLE_TASK: &str = include_str!("sql/semantic_durable_task.sql");
pub(super) const OVERAGE: &str = include_str!("sql/overage.sql");
