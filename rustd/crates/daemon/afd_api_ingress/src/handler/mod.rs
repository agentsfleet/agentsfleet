//! Ingress-plane handler families.

pub(crate) mod events;
mod mention;
pub(crate) mod webhook;

pub(crate) use afd_http::handler::{Refusal, provider_of};
