//! Durable objective context carried by the existing lease protocol.
//!
//! A task is an owner across leases. Its generation, a lease fence and a
//! checkpoint generation remain separate counters with separate purposes.

mod context;
mod limits;
mod state;

pub use context::TaskContext;
pub use limits::{TaskLimits, TaskUsage};
pub use state::TaskState;

/// Maximum unfinished objectives a fleet can retain.
pub const UNFINISHED_MAX: u32 = 16;
/// Maximum human admissions read at one safe boundary.
pub const INPUT_BATCH_MAX: usize = 4;
/// Maximum immutable interpreted actions from one admitted message.
pub const ITEMS_MAX: usize = 4;
/// Maximum UTF-8 bytes in an accepted objective.
pub const OBJECTIVE_MAX_BYTES: usize = 16 * 1024;
/// Maximum UTF-8 bytes in a compact task title.
pub const TITLE_MAX_BYTES: usize = 160;
/// Maximum semantic completion questions accepted for one objective.
pub const QUESTIONS_MAX: usize = 6;
/// Maximum UTF-8 bytes in one completion question.
pub const QUESTION_MAX_BYTES: usize = 1024;
/// Runner capability required before task-bound work may be issued.
pub const CAPABILITY: &str = "durable_tasks_v1";
