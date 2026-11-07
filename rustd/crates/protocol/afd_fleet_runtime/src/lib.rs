//! Fleet configuration: the native setup Definition, parsed once into the
//! typed policy every gate reads.
//!
//! # What this crate is for
//!
//! A fleet's behaviour is authored in chat as a typed JSON Definition — a
//! description, instructions and a configuration document — and the configuration
//! is stored as the `config` key of the head `fleet.revisions.definition`. At
//! claim time the daemon has to turn that document into decisions: what may
//! wake this fleet, what it may spend, which of its actions need a human, how
//! far its credentials reach.
//! This crate is that transformation, and nothing else. It opens no socket,
//! touches no datastore, and holds no clock.
//!
//! # Where the shape work went
//!
//! Most of a configuration parser is shape work — fetch a key or fail, branch
//! per value, copy each string, and unwind a partially-built struct when a
//! later key fails. serde does all of it from a type declaration, and
//! ownership does the unwinding at scope exit.
//!
//! What is left is the half that is actually about fleets, and it reads as
//! that: a name is a kebab slug, a ceiling is positive and finite, two webhook
//! triggers may not share a source, a repository binding is optional as a WHOLE
//! rather than key by key.
//!
//! Four verdicts are worth knowing before reading a refusal, and each is
//! declared in [`error`]: a malformed field never reports as a missing one, a
//! non-string `model` is never silently dropped, a gate failure keeps its own
//! class, and a threshold is bounded by a constant named for what it bounds.
//! [`Definition`] adds one more: a key outside the supported setup is refused,
//! because setup a reviewer cannot see must not reach a runner.
//!
//! # What it deliberately does not do
//!
//! It does not verify a webhook signature and it does not call a provider. It
//! reads `afd_webhook::Scheme`'s source names to refuse a webhook trigger no
//! route could verify; the verification itself lives at the ingress, so a
//! config parse never drags the verification path in behind it.

#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

// The schema-parity suite's pattern matcher, used by `tests/` only.
#[cfg(test)]
use regex as _;
// The stored fixture setup `test_util`'s pin reads; a build without the
// feature compiles that pin out.
#[cfg(all(test, not(feature = "test-util")))]
use afd_db as _;

pub mod config;
pub mod definition;
pub mod error;
pub mod name;
#[cfg(feature = "openapi")]
mod openapi;
mod optional;
#[cfg(feature = "test-util")]
pub mod test_util;

pub use self::config::FleetConfig;
pub use self::definition::Definition;
pub use self::error::{Error, Result};
pub use self::name::{CredentialName, FleetName};
