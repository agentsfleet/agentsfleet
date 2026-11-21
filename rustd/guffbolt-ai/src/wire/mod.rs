//! The wires: one model's [`Route`], the [`transport::Send`] its requests go
//! through, and [`Connected`], the [`Model`](crate::Model) that streams its
//! turns.
//!
//! ```text
//!  Connected::stream(Request)
//!    ├─ request:  Request ──► rig's CompletionRequest for the route's Api
//!    ├─ count:    measured locally; near the window, confirmed by the
//!    │            provider's own count endpoint where it has one
//!    ├─ stream:   rig encodes, Send carries it (Retry before any byte is
//!    │            read), rig decodes events under the idle deadline
//!    └─ reopen:   a turn the provider ended early, before anything visible
//!                 went out and before any spend was known, opens again
//! ```
//!
//! rig speaks every wire. This module owns what rig does not: the transport
//! seam, its retry, the byte bound an embedder may set, the deadlines a turn
//! is read under, and the chunks a turn becomes.

mod budget;
mod connected;
mod count;
mod document;
mod image;
mod json;
mod observe;
#[cfg(test)]
mod recorder;
mod request;
mod retry;
mod route;
mod stream;
mod timeouts;
pub mod transport;
mod usage;

pub use self::budget::{estimate, input_tokens};
pub use self::connected::Connected;
pub use self::observe::{Calibration, FailureCause, ObservedTokens, Observer, TurnObservation};
pub use self::request::context_document;
pub use self::retry::{Retry, Retrying, retry_after};
pub use self::route::{Api, Route};
pub use self::timeouts::Timeouts;
