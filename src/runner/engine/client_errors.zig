//! Engine error set + error-code string constants.
//! Kept from the former client.zig split; the RPC-error-code mapping
//! functions were removed with the socket layer (no protocol.zig caller).

pub const ClientError = error{
    ConnectionFailed,
    TransportLoss,
    ExecutionFailed,
    InvalidResponse,
    SessionNotFound,
    LeaseExpired,
    PolicyDenied,
    TimeoutKilled,
    OomKilled,
    ResourceKilled,
    LandlockDenied,
};

// Error-code mirrors of src/errors/error_registry.zig — the engine
// binary tree forbids imports outside src/runner/ (build_runner.zig keeps the
// engine portable) so the canonical strings are duplicated here. Every engine
// source needing a UZ-EXEC-* / UZ-TOOL-* literal MUST import from this file —
// never declare a local `const ERR_X` in another engine source file.
// `audit-error-codes.sh --strict` flags raw `"UZ-…"` literals outside this file.
pub const ERR_EXEC_SESSION_CREATE_FAILED: []const u8 = "UZ-EXEC-001";
pub const ERR_EXEC_STAGE_START_FAILED: []const u8 = "UZ-EXEC-002";
pub const ERR_EXEC_TIMEOUT_KILL: []const u8 = "UZ-EXEC-003";
pub const ERR_EXEC_OOM_KILL: []const u8 = "UZ-EXEC-004";
pub const ERR_EXEC_RESOURCE_KILL: []const u8 = "UZ-EXEC-005";
pub const ERR_EXEC_TRANSPORT_LOSS: []const u8 = "UZ-EXEC-006";
pub const ERR_EXEC_LEASE_EXPIRED: []const u8 = "UZ-EXEC-007";
pub const ERR_EXEC_STARTUP_POSTURE: []const u8 = "UZ-EXEC-009";
pub const ERR_EXEC_CRASH: []const u8 = "UZ-EXEC-010";
pub const ERR_EXEC_LANDLOCK_DENY: []const u8 = "UZ-EXEC-011";
pub const ERR_EXEC_RUNNER_AGENT_INIT: []const u8 = "UZ-EXEC-012";
pub const ERR_EXEC_RUNNER_AGENT_RUN: []const u8 = "UZ-EXEC-013";
pub const ERR_EXEC_RUNNER_INVALID_CONFIG: []const u8 = "UZ-EXEC-014";
pub const ERR_TOOL_UNKNOWN: []const u8 = "UZ-TOOL-005";

