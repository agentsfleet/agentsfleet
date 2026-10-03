//! The model providers: one [`Provider`] trait, the turn it streams, and the
//! wires behind it (`docs/architecture/runner_execution.md` §Crates).
//!
//! A [`Connect`] picks the wire a lease's policy names and builds its
//! provider. Every wire shares one transport, generic over the wire's
//! dialect: one HTTP client, bounded retry, and Server-Sent Events framing.
//! The model key lives only here, in the supervisor: no provider runs inside
//! a sandbox.

pub mod error;

mod anthropic;
mod connect;
mod dialect;
mod http;
mod openai_chat;
mod openai_responses;
mod provider;
mod retry;
mod sse;
#[cfg(test)]
mod test_support;

pub use self::connect::{Connect, Connector, Endpoints};
pub use self::error::{Error, Result};
pub use self::provider::{Call, Chunk, Message, Provider, Request, Usage};
