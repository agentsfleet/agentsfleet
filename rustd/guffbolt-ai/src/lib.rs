//! The provider side of the guffbolt agent harness: what one model call is
//! asked, the chunks its stream answers with, and the [`Model`] trait a
//! provider implements.
//!
//! ```text
//!  guffbolt: the loop ──► Request ──► Model::stream ──► Chunk … Chunk::End
//!                                       ▲
//!  a provider crate: implements Model over its own wire
//!  feature `wire`:   wire::Connected implements it over rig's wires
//! ```
//!
//! guffbolt depends on this crate and re-exports every type here under its
//! own path, so a provider needs this crate alone and a loop needs guffbolt.
//! A step's [`Replay`] is the one provider-owned value: the loop keeps it with
//! the step and hands it back unopened.
//!
//! The default build is the types alone. Feature `wire` adds the wires:
//! a [`wire::Route`] names the API, base URL and limits of one model, and a
//! [`wire::Connected`] streams its turns through a [`wire::transport::Send`]
//! the embedder may replace. Feature `reqwest` adds the default transport,
//! `log-filter` the filter that keeps rig's own lines quiet, and `test-util`
//! a fake provider's TLS listener.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod types;

#[cfg(feature = "wire")]
mod error;
#[cfg(feature = "log-filter")]
mod logs;
#[cfg(feature = "test-util")]
pub mod testing;
#[cfg(feature = "wire")]
pub mod wire;

pub use self::types::{
    Call, Chunk, End, Hosted, Image, ImageKind, Message, Model, Replay, Request, TextKind,
    ToolSpec, Usage,
};

#[cfg(feature = "wire")]
pub use self::error::{Error, Result};
#[cfg(feature = "log-filter")]
pub use self::logs::log_filter;
