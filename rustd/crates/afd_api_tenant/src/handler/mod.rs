//! Tenant-plane handler families.

pub(crate) mod approval;
pub(crate) mod auth;
pub(crate) mod connector;
pub(crate) mod event;
pub mod fleet;
pub(crate) mod fleet_bundles;
pub(crate) mod grant;
pub(crate) mod preference;
pub(crate) mod schedule;
pub mod secret;
pub(crate) mod stream;
pub mod tenant;
pub mod workspace_library;

pub(crate) use afd_http::handler::{
    BrokenEscape, Refusal, decoded_parameter, library_onboard, malformed, parameter, provider_of,
    refuse,
};
