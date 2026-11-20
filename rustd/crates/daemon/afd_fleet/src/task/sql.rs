//! Statements owned by the task lifecycle, with the run owner reused.

pub(super) const LOCK_THREAD: &str = include_str!("sql/lock_thread.sql");
pub(super) const RUN: &str = include_str!("sql/run.sql");
pub(super) const LOCK_REPLAY_THREAD: &str = include_str!("sql/lock_replay_thread.sql");
pub(super) const REPLAY_RUN: &str = include_str!("sql/replay_run.sql");
pub(super) const LOCK_SOURCE: &str = include_str!("sql/lock_source.sql");
pub(super) const REPLAY: &str = include_str!("sql/replay.sql");
pub(super) const INSERT_ITEM: &str = include_str!("sql/insert_item.sql");
pub(super) const INSERT_TASK: &str = include_str!("sql/insert_task.sql");
pub(super) const BIND_RUN: &str = include_str!("sql/bind_run.sql");
pub(super) const READ: &str = include_str!("sql/read.sql");
pub(super) const LOCK_TASK: &str = include_str!("sql/lock_task.sql");
pub(super) const CONTROL: &str = include_str!("sql/control.sql");

pub(super) const RUN_SCOPE: &str = include_str!("sql/run_scope.sql");
pub(super) const LOCK_SCOPE: &str = include_str!("sql/lock_scope.sql");
pub(super) const LOCK_ISSUE_SCOPE: &str = include_str!("sql/lock_issue_scope.sql");
pub(super) const LOCK_WORKSPACE: &str = include_str!("checkpoint/sql/lock_workspace.sql");
pub(super) const COLLECT_COMPLETIONS: &str = include_str!("waits/sql/collect_completions.sql");
pub(super) const LOCK_SCOPE_THREAD: &str = include_str!("sql/lock_scope_thread.sql");

pub(super) const BIND_TURN: &str = include_str!("sql/bind_turn.sql");

pub(super) const CONTROL_ORIGIN: &str = include_str!("sql/control_origin.sql");
pub(super) const CONTROL_TURN: &str = include_str!("sql/control_turn.sql");
