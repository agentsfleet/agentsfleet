//! `/v1/api-keys*` and the rest of what a tenant manages for itself.
//!
//! Thin by construction, like [`super::auth`]: parse the body into a type that
//! already carries its bounds, resolve which tenant is acting, call one service
//! method, render. No handler here decides a status, a scope, or whose row it
//! is reading — the first is a property of the error's code, the second is the
//! route table's, and the third is the tenant the credential resolved to.

mod api_key;

pub(crate) use self::api_key::{delete, list, mint, revoke};
