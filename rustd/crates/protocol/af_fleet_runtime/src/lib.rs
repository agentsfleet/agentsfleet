//! Fleet configuration requests, parsed once.
//!
//! The triggers that may wake a fleet, the GitHub scope a grant reaches, where
//! its HTTP tools may go, its name, and the completion statements a durable
//! task is judged against.
//!
//! # What this crate is for
//!
//! A fleet is a row, its grants, its triggers and its schedules. Each of those
//! arrives as a request a run or a person sends, and is stored as a row; this
//! crate is the one parser both directions go through, so a trigger the daemon
//! stores is one it would accept again. It opens no socket, touches no
//! datastore, and holds no clock.
//!
//! # Where the shape work went
//!
//! Most of a configuration parser is shape work — fetch a key or fail, branch
//! per value, copy each string. serde does all of it from a type declaration.
//! What is left is the half that is actually about fleets: a name is a kebab
//! slug, two webhook triggers may not share a source, a GitHub scope names both
//! its repositories and how far it reaches.
//!
//! # What it deliberately does not do
//!
//! It does not verify a webhook signature and it does not call a provider. It
//! reads `af_webhook::Scheme`'s source names to refuse a webhook trigger no
//! route could verify; the verification itself lives at the ingress.

#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

pub mod config;
pub mod error;
pub mod name;
mod optional;

pub use self::error::{Error, Result};
pub use self::name::{CredentialName, FleetName};
