//! Auth-namespaced error-code re-exports.
//!
//! Pre-v2 RULE NLG forbids string-literal duplication across layers; this
//! module re-exports from `src/errors/error_registry.zig` (the canonical
//! source) so middleware callers keep their `errors.ERR_X` namespacing
//! without redeclaring the values. Drift is impossible because there's
//! nothing to drift — both names resolve to the same comptime constant.

const registry = @import("../../errors/error_registry.zig");

pub const ERR_FORBIDDEN = registry.ERR_FORBIDDEN;
pub const ERR_UNAUTHORIZED = registry.ERR_UNAUTHORIZED;
pub const ERR_TOKEN_EXPIRED = registry.ERR_TOKEN_EXPIRED;
pub const ERR_AUTH_UNAVAILABLE = registry.ERR_AUTH_UNAVAILABLE;
pub const ERR_INSUFFICIENT_ROLE = registry.ERR_INSUFFICIENT_ROLE;
pub const ERR_UNSUPPORTED_ROLE = registry.ERR_UNSUPPORTED_ROLE;
pub const ERR_APPROVAL_INVALID_SIGNATURE = registry.ERR_APPROVAL_INVALID_SIGNATURE;
// Generic webhook signature codes — shared by webhook_sig and svix_signature.
// Named without provider prefix because the same code fires for every
// HMAC-style webhook failure.
pub const ERR_WEBHOOK_SIG_INVALID = registry.ERR_WEBHOOK_SIG_INVALID;
pub const ERR_WEBHOOK_TIMESTAMP_STALE = registry.ERR_WEBHOOK_TIMESTAMP_STALE;
pub const ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED = registry.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED;
