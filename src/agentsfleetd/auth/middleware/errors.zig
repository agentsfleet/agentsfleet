//! Auth-namespaced error-code re-exports.
//!
//! Middleware callers keep their `errors.ERR_X` namespacing without
//! redeclaring values. The source is the `auth_codes` named-module leaf —
//! not a relative import of `errors/error_registry.zig`, which would escape
//! the folder and fail the `test-auth` portability gate. The leaf mirrors the
//! canonical literals; `error_registry.zig` pins them byte-equal at comptime,
//! so drift is a compile error.

const auth_codes = @import("auth_codes");

pub const ERR_FORBIDDEN = auth_codes.ERR_FORBIDDEN;
pub const ERR_UNAUTHORIZED = auth_codes.ERR_UNAUTHORIZED;
pub const ERR_TOKEN_EXPIRED = auth_codes.ERR_TOKEN_EXPIRED;
pub const ERR_AUTH_UNAVAILABLE = auth_codes.ERR_AUTH_UNAVAILABLE;
pub const ERR_INSUFFICIENT_SCOPE = auth_codes.ERR_INSUFFICIENT_SCOPE;
pub const ERR_APPROVAL_INVALID_SIGNATURE = auth_codes.ERR_APPROVAL_INVALID_SIGNATURE;
// Generic webhook signature codes — shared by webhook_sig and svix_signature.
// Named without provider prefix because the same code fires for every
// HMAC-style webhook failure.
pub const ERR_WEBHOOK_SIG_INVALID = auth_codes.ERR_WEBHOOK_SIG_INVALID;
pub const ERR_WEBHOOK_TIMESTAMP_STALE = auth_codes.ERR_WEBHOOK_TIMESTAMP_STALE;
pub const ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED = auth_codes.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED;
// Runner plane — unknown tokens stay invalid; known non-active runners are distinct.
pub const ERR_RUN_INVALID_RUNNER_TOKEN = auth_codes.ERR_RUN_INVALID_RUNNER_TOKEN;
pub const ERR_RUN_ADMIN_STATE_BLOCKED = auth_codes.ERR_RUN_ADMIN_STATE_BLOCKED;
