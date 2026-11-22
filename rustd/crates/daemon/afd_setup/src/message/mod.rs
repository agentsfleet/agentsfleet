//! Durable message requests; Stop is the turn PATCH every thread shares.

mod accept;
mod conversation;
mod read;
mod stop;

pub(crate) use accept::{Insertion, insert};
pub use conversation::Conversation;

pub(crate) const ACCEPTED: &str = "accepted";

pub(crate) use af_fleet_runtime::definition::normalize_text as normalize;
