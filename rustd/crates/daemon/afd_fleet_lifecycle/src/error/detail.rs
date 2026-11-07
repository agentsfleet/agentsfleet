//! The sentence each refusal tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! A client and a dashboard match on some of these strings, so the bytes are a
//! wire fact rather than prose this crate is free to improve.

/// The detail a request earns when the database cannot be reached.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail a request earns when a statement fails.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// A fleet id this workspace does not hold.
pub const NOT_FOUND: &str = "Fleet not found";

/// An edit guarded on a source that has since changed.
pub const SOURCE_STALE: &str =
    "The fleet changed since you read it; refetch and send its current tag";

/// A delete of a fleet that is not yet killed.
pub const MUST_KILL_FIRST: &str = "Fleet must be killed before delete (PATCH status=killed first)";
