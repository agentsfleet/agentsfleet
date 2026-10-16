//! Schedule edits hold the parent and child until intent and sync ownership agree.

mod guard;
mod input;
mod sql;
mod write;

use afd_api_wire::schedule::Patch;
use afd_core::{clock::UnixMillis, id::Uuid7};

/// One conditional edit, with the workspace proven by the mounted route.
#[derive(Debug, Clone, Copy)]
pub struct EditRequest<'a> {
    /// Workspace the caller may edit.
    pub workspace: &'a Uuid7,
    /// Fleet whose setup also serializes recipe replacement.
    pub fleet: &'a Uuid7,
    /// Schedule under that fleet.
    pub schedule: &'a Uuid7,
    /// Required version from the last resource read.
    pub expected: Option<&'a str>,
    /// An overlay or an independent schedule edit.
    pub patch: &'a Patch,
    /// Audit time; fence expiry uses the database clock.
    pub now: UnixMillis,
}
