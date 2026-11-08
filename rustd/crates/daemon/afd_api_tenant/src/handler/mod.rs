//! Tenant-plane handler families.

pub(crate) mod approval;
pub(crate) mod auth;
pub(crate) mod connector;
pub(crate) mod draft;
pub(crate) mod event;
pub mod fleet;
pub(crate) mod grant;
pub(crate) mod input;
pub(crate) mod library;
pub(crate) mod paging;
pub(crate) mod preference;
pub(crate) mod schedule;
pub mod secret;
pub(crate) mod stream;
pub mod tenant;

pub(crate) use afd_http::handler::{
    BrokenEscape, Refusal, decoded_parameter, malformed, parameter, provider_of, refuse,
};
