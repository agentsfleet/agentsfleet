//! Private chat setup over the shared native Definition.
//!
//! Access is checked inside each transaction, before replay. Candidates are
//! immutable snapshots; publishing and activation copy them into shared setup.
//! HTTP adapters and the runner use this domain owner without owning its rows.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod access;
pub mod billing_sweep;
mod candidate;
mod catalogue;
mod creation;
mod draft;
pub mod error;
mod funnel;
mod lease;
mod library;
mod maintenance;
mod message;
mod model;
mod observe;
mod paging;
mod privacy;
mod replay;
mod review;
mod setup;
mod sql;
pub mod types;

pub use self::access::Access;
pub use self::creation::{Creations, SavedFleet, SetupCheck, SetupProgress, SystemSetup};
pub use self::draft::Drafts;
pub use self::error::{Error, Result};
pub use self::lease::{ActivityInput, Claimed};
pub use self::maintenance::Maintenance;
pub use self::message::{Cancellation, Stopped};
pub use self::model::{
    CONTEXT_EVENT, ContextNotice, DELTA_EVENT, Delta, ModelCallInput, ModelCallResult, ModelCalls,
    Network, ProviderRequest, ProviderResponse, Transport, draft_channel,
};
pub use self::observe::{Notice, Observation, View};
pub use self::setup::Setup;
