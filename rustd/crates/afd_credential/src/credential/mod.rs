//! The on-demand credential broker.
//!
//! A sandboxed child asks its runner for a short-lived token at the moment a
//! tool needs it, and the runner forwards the ask over the `agt_r` plane. What
//! arrives here is a lease id and an integration name; what leaves is a token
//! that outlives neither.

pub mod broker;
pub mod github;
pub mod oauth;
pub mod outcome;
pub mod platform;

pub use self::broker::{Ask, Broker, Exchanger, Vendors};
pub use self::outcome::{Minted, Outcome, Retry};
