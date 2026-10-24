//! Model context is replaceable working state; the authoritative history is not.

mod checkpoint;
mod compact;
mod error;
mod plan;
mod ports;
mod private_debug;
mod summary;
mod window;

#[cfg(feature = "postgres")]
pub mod repository;

pub use checkpoint::{Checkpoint, Identity, digest_groups};
pub use compact::{Compaction, Progress, execute};
pub use error::{Error, Result};
pub use plan::{Group, HISTORY_PAGE_SIZE, Page, Plan, prepare};
pub use ports::{CheckpointStore, HistorySource, PortFuture, Summarizer, TokenEstimator};
pub use summary::{SUMMARY_INSTRUCTIONS, SUMMARY_OUTPUT_TOKENS, Summary};
pub use window::{COMPACT_PERCENT, SAFE_PERCENT, Window};
