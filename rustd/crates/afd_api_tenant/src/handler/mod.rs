//! Tenant-plane handler families.

pub(crate) mod approval;
pub(crate) mod auth;
pub(crate) mod event;
pub mod fleet;
pub(crate) mod fleet_bundles;
pub(crate) mod grant;
pub(crate) mod preference;
pub mod secret;
pub(crate) mod stream;
pub mod tenant;

pub(crate) use afd_http::handler::{Refusal, decoded_parameter, malformed, parameter, refuse};
