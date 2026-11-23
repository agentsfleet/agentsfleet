//! The largest request body a route reads: `[server] max_request_body_mib`.
//!
//! One bound for every body that reaches this daemon over the network, laid
//! over each route group as axum's `DefaultBodyLimit`, so a body past it is
//! refused 413 before a handler reads it. A route that carries text at the
//! one text ceiling, or a runner frame, keeps the larger limit derived from
//! that ceiling.

/// One mebibyte.
pub const MIB: usize = 1024 * 1024;

/// The bound when the daemon's file names none.
pub const DEFAULT_MAX_REQUEST_BODY_BYTES: usize = MIB;
