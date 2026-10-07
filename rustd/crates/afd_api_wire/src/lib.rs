//! The daemon's own API types: every admin, tenant, operator and ingress shape
//! `agentsfleetd` serves and no runner speaks.
//!
//! Split from `afd_wire` by consumer: `afd_wire` keeps what the daemon and
//! the runner both speak; this crate depends on it for the shared
//! types its shapes embed, and nothing in `afd_wire` names this crate, so an
//! edit here rebuilds no runner crate.
//!
//! The encoding rules are `afd_wire`'s and hold here unchanged: borrowed
//! `Cow<'a, str>` text, primitives rather than validated newtypes, no
//! `skip_serializing_if`, and schemas behind the non-default `openapi` feature.

// Same reason as `afd_wire`: an unused-but-linked dependency is a cost with no
// benefit. Gated on `not(test)` because the test build links dev-dependencies.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod admin;
mod admin_catalogue;
mod admin_library;
pub mod approval;
pub mod auth;
pub mod connector;
pub mod fleet;
pub mod grant;
pub mod health;
pub mod identity;
pub mod ingress;
pub mod models;
pub mod operator;
pub mod preference;
mod redact;
pub mod schedule;
pub mod schema;
pub mod secret;
pub mod tail;
pub mod team;
pub mod tenant;
pub mod tenant_model_entry;
pub mod tenant_provider;
pub mod workspace;
pub mod workspace_library;
