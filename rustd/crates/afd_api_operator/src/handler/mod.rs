//! Operator-plane handler families.

pub(crate) mod admin;
pub(crate) mod operator;

pub(crate) use afd_http::handler::{library_onboard, malformed, refuse, reject};
