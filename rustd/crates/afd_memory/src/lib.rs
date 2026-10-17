//! Fleet memory in Postgres.
//!
//! [`PgStore`] is where every memory read and write in `agentsfleetd` lands.
//! [`Memories`] is the handle the daemon holds once: it reads each fleet's
//! workspace and shared-memory grants from `core.fleets`, applies them, and
//! hands the call to the store.
//!
//! # Sharing is a grant, and the writer is in every identity
//!
//! An entry belongs to the fleet that wrote it, under `(workspace, fleet,
//! key)`. A fleet holding the publish grant may mark an entry
//! [`Visibility::Workspace`](afd_wire::memory::Visibility); a fleet holding the
//! read grant hydrates and recalls those entries from the workspace's other
//! fleets, each naming its writer. No fleet overwrites or forgets another's.
//! `docs/architecture/runner_fleet.md` §"Memory backends and scope" is the
//! decision; this crate is where it is enforced.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod access;
mod admit;
pub mod error;
mod hydrate;
mod memories;
pub mod page;
mod postgres;
mod record;
mod sql;
pub mod window;

pub use self::error::{Error, Result};
pub use self::memories::{Captured, Memories};
pub use self::postgres::PgStore;
pub use self::record::{Housekept, Owner, Record};
