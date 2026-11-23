//! Statements stay with their domain owner; values and lifecycle words are bound.

pub(crate) mod access;
pub(crate) mod candidate;
pub(crate) mod draft;
pub(crate) mod message;

/// Opens a read's transaction on one snapshot that takes no row lock: every
/// statement in it sees the same committed state, and none of them waits on,
/// or holds up, a writer. Postgres refuses a locking read inside it.
pub(crate) const SNAPSHOT: &str = include_str!("snapshot.sql");
