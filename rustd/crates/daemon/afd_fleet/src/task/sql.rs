//! Statements owned by the task lifecycle, with the lease owner reused.

pub(super) const LOCK_SLOT: &str = include_str!("sql/lock_slot.sql");
pub(super) const LEASE: &str = include_str!("sql/lease.sql");
pub(super) const LOCK_REPLAY_SLOT: &str = include_str!("sql/lock_replay_slot.sql");
pub(super) const REPLAY_LEASE: &str = include_str!("sql/replay_lease.sql");
pub(super) const LOCK_SOURCE: &str = include_str!("sql/lock_source.sql");
pub(super) const REPLAY: &str = include_str!("sql/replay.sql");
pub(super) const INSERT_ITEM: &str = include_str!("sql/insert_item.sql");
pub(super) const INSERT_TASK: &str = include_str!("sql/insert_task.sql");
pub(super) const BIND_LEASE: &str = include_str!("sql/bind_lease.sql");
pub(super) const READ: &str = include_str!("sql/read.sql");
pub(super) const LOCK_TASK: &str = include_str!("sql/lock_task.sql");
pub(super) const CONTROL: &str = include_str!("sql/control.sql");

pub(super) const LEASE_SCOPE: &str = include_str!("sql/lease_scope.sql");
pub(super) const LOCK_SCOPE: &str = include_str!("sql/lock_scope.sql");
pub(super) const LOCK_ISSUE_SCOPE: &str = include_str!("sql/lock_issue_scope.sql");
pub(super) const LOCK_WORKSPACE: &str = include_str!("checkpoint/sql/lock_workspace.sql");
pub(super) const COLLECT_COMPLETIONS: &str = include_str!("waits/sql/collect_completions.sql");
pub(super) const LOCK_SCOPE_SLOT: &str = include_str!("sql/lock_scope_slot.sql");

pub(super) const BIND_ADMISSION: &str = include_str!("sql/bind_admission.sql");

pub(super) const CONTROL_ORIGIN: &str = include_str!("sql/control_origin.sql");
pub(super) const CONTROL_ADMISSION: &str = include_str!("sql/control_admission.sql");
