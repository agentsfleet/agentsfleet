//! The sentence each refusal tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! A client and a dashboard match on some of these strings, so the bytes are a
//! wire fact rather than prose this crate is free to improve.

/// The detail a request earns when the database cannot be reached.
pub(crate) use af_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail a request earns when a statement fails.
pub(crate) use af_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// A fleet id this workspace does not hold.
pub const NOT_FOUND: &str = "Fleet not found";

/// A fleet whose purge found a schedule still registered with the external
/// scheduler: the one sentence `afd_cron`'s release answers too.
pub const SCHEDULES_REGISTERED: &str = af_core::error::DETAIL_SCHEDULES_REGISTERED;
