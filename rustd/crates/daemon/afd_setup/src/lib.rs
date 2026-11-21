//! Private chat setup over the shared native Definition.
//!
//! Access is checked inside each transaction, before replay. Candidates are
//! immutable snapshots; Save and activation copy them into shared setup.
//! HTTP adapters and the runner use this domain owner without owning its rows.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod access;
mod candidate;
mod catalogue;
pub mod context;
mod creation;
mod draft;
pub mod error;
mod funnel;
#[cfg(test)]
mod limits;
mod maintenance;
mod message;
mod observe;
mod paging;
mod privacy;
mod replay;
mod review;
mod setup;
pub mod setup_run;
mod sql;
pub mod types;

pub use self::access::Access;
pub use self::creation::{Creations, SavedFleet, SetupCheck, SetupProgress, SystemSetup};
pub use self::draft::Drafts;
pub use self::error::{Error, Result};
pub use self::maintenance::Maintenance;
pub use self::message::Conversation;
pub use self::observe::{Notice, Observation, View};
pub use self::setup::Setup;
pub use self::setup_run::{CONTEXT_EVENT, ContextNotice, DELTA_EVENT, Delta, TextKind};
