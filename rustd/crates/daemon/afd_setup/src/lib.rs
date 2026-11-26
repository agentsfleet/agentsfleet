//! A workspace's fleets as a person creates, reads, changes and deletes them.
//!
//! A person's first message creates an active fleet with its thread and first
//! turn in one transaction; nothing is set up before it works. Access is
//! checked inside each transaction, before replay. This crate also keeps the
//! fleet chat's compaction store ([`context`]), the workspace's model choices
//! and a fleet's trigger rows ([`trigger`]). HTTP adapters and the run plane
//! use it without owning its rows.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod access;
mod catalogue;
pub mod context;
mod creation;
pub mod error;
#[cfg(test)]
mod limits;
mod replay;
mod sql;
pub mod trigger;
pub mod tuning;
pub mod types;

pub use self::access::Access;
pub use self::creation::{Creations, HeldDelete};
pub use self::error::{Error, Result};
