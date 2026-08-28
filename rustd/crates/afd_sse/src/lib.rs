//! One Server-Sent Events connection: what it carries, and how many of them
//! this instance holds.
//!
//! # Why this is not a module in `afd_api`
//!
//! Nothing here needs a router. A frame's shape, the sequence discipline behind
//! it, the rule that a frame the multiplex cannot route is dropped, and the
//! ceiling on concurrent streams are all decided with no request in sight — and
//! all of them are the parts worth proving. Leaving them in the HTTP crate
//! would mean every one of those proofs had to build a router first, and
//! `afd_api` is already the largest crate in this workspace.
//!
//! # What stays in `afd_api`
//!
//! Rendering. A [`Frame`] is `seq`, `kind` and `data`; turning that into
//! `id:`/`event:`/`data:` lines on a socket is `axum::response::sse`'s job, and
//! so is the heartbeat comment that probes a vanished client. This crate names
//! the cadence and leaves the writing to the library that owns the transport.
//!
//! # What stays in the caller
//!
//! Postgres. The fan-in is told which fleets to carry; it does not enumerate
//! them, and it never re-authorizes a caller. Both are the handler's, because
//! both are reads against a workspace this crate has no business holding a pool
//! for.

pub mod ceiling;
pub mod channel;
pub mod error;
pub mod fanin;
pub mod frame;
pub mod live;
pub mod tail;

use std::time::Duration;

pub use crate::ceiling::{Ceiling, Slot};
pub use crate::error::{Error, Result};
pub use crate::fanin::{Delta, FanIn};
pub use crate::frame::{DEFAULT_KIND, Frame, KIND_CATCHING_UP, KIND_HELLO};
pub use crate::live::Live;
pub use crate::tail::tail;

/// How often a stream with nothing to say says nothing, out loud.
///
/// `HEARTBEAT_INTERVAL_MS`, mirrored. The write is the point: it is what
/// discovers a client that went away without closing, and without it a stream
/// over a dead connection would hold its slot until a publish that may never
/// come. It also keeps intermediaries from idling the connection out.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// The text of the heartbeat comment.
///
/// A comment rather than a frame, so an `EventSource` ignores it and no client
/// has to learn a keep-alive event name.
pub const HEARTBEAT_TEXT: &str = "heartbeat";
