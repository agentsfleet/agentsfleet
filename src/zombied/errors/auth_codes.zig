//! Auth-plane error-code mirror leaf.
//!
//! The auth middleware (`src/zombied/auth/**`) must compile in isolation: the
//! `test-auth` portability gate links only `auth/**` plus named modules, so it
//! cannot relative-import `errors/error_registry.zig` — that escapes the folder
//! and fails the gate. This leaf is a named module holding the exact code
//! strings the auth plane references, importable by name from inside the gate.
//!
//! Deliberate mirror, same shape as `runner/engine/client_errors.zig`: the
//! canonical registry keeps these literals too, and an `inline for` in
//! `error_registry.zig` asserts byte-equality at comptime — drift is a compile
//! error, not a discipline risk.

pub const ERR_FORBIDDEN = "UZ-AUTH-001";
pub const ERR_UNAUTHORIZED = "UZ-AUTH-002";
pub const ERR_TOKEN_EXPIRED = "UZ-AUTH-003";
pub const ERR_AUTH_UNAVAILABLE = "UZ-AUTH-004";
pub const ERR_INSUFFICIENT_ROLE = "UZ-AUTH-009";
pub const ERR_UNSUPPORTED_ROLE = "UZ-AUTH-010";
pub const ERR_PLATFORM_ADMIN_REQUIRED = "UZ-AUTH-021";
pub const ERR_APPROVAL_INVALID_SIGNATURE = "UZ-APPROVAL-003";
// Generic webhook signature codes — one set for every HMAC-style webhook.
pub const ERR_WEBHOOK_SIG_INVALID = "UZ-WH-010";
pub const ERR_WEBHOOK_TIMESTAMP_STALE = "UZ-WH-011";
pub const ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED = "UZ-WH-020";
pub const ERR_APIKEY_REVOKED = "UZ-APIKEY-004";
// Runner plane — unknown tokens stay invalid; known non-active runners are distinct.
pub const ERR_RUN_INVALID_RUNNER_TOKEN = "UZ-RUN-001";
pub const ERR_RUN_ADMIN_STATE_BLOCKED = "UZ-RUN-009";
