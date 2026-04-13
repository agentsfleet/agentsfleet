//! Auth error-code constants used by the middleware layer.
//!
//! Duplicated here (not imported from `src/errors/`) so `src/auth/` stays
//! portable (§1.2). `src/errors/error_registry.zig` is the source of truth
//! for HTTP status + docs_uri + title; these strings must stay in sync.
//! A cross-layer parity test (landing in Batch D) asserts that.

pub const ERR_UNAUTHORIZED: []const u8 = "UZ-AUTH-002";
pub const ERR_TOKEN_EXPIRED: []const u8 = "UZ-AUTH-003";
pub const ERR_AUTH_UNAVAILABLE: []const u8 = "UZ-AUTH-004";
pub const ERR_INSUFFICIENT_ROLE: []const u8 = "UZ-AUTH-009";
pub const ERR_UNSUPPORTED_ROLE: []const u8 = "UZ-AUTH-010";
