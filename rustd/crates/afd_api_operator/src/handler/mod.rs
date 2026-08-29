//! Operator-plane handler families.

pub(crate) mod admin;
pub(crate) mod operator;

pub(crate) use afd_http::handler::{malformed, refuse, reject};
