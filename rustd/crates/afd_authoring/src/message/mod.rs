//! Durable message requests and monotone cancellation; no transient turn resource.

mod accept;
mod cancel;
mod read;

pub(crate) use accept::{Insertion, insert};
pub use cancel::{Cancellation, Stopped};

pub(crate) const ACCEPTED: &str = "accepted";

pub(crate) use afd_fleet_runtime::definition::normalize_text as normalize;
