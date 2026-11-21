//! The provider side of the guffbolt agent harness: what one model call is
//! asked, the chunks its stream answers with, and the [`Model`] trait a
//! provider implements.
//!
//! ```text
//!  guffbolt: the loop ──► Request ──► Model::stream ──► Chunk … Chunk::End
//!                                       ▲
//!  a provider crate: implements Model over its own wire
//! ```
//!
//! guffbolt depends on this crate and re-exports every type here under its
//! own path, so a provider needs this crate alone and a loop needs guffbolt.
//! A step's [`Replay`] is the one provider-owned value: the loop keeps it with
//! the step and hands it back unopened.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod types;

pub use self::types::{
    Call, Chunk, End, Hosted, Image, ImageKind, Message, Model, Replay, Request, TextKind,
    ToolSpec, Usage,
};
