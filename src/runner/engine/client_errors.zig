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

// Error-code mirrors of src/zombied/errors/error_registry.zig — the runner
// binary tree forbids imports into src/zombied/ (build_runner.zig keeps the
// runner portable) so the canonical strings are duplicated here. Every runner
// source needing a UZ-EXEC-* / UZ-TOOL-* / UZ-RUN-* literal MUST import from
// this file — never declare a local `const ERR_X` in another runner source.
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

// Fleet control-plane code the parent supervisor emits when a lease's mandatory
// sandbox cannot be established and the lease is refused unrun (Invariant 7).
pub const ERR_RUN_SANDBOX_ESTABLISH_FAILED: []const u8 = "UZ-RUN-007";

// Fleet control-plane codes the runner OBSERVES on the wire and acts on: 009 on
// a heartbeat/lease/renew call after an operator revoke (→ drain/stop); 010/011
// from a `/renew` response (→ kill the child: cap reached, or the lease was
// reassigned to another runner); 012 when the tenant can no longer fund the run.
pub const ERR_RUN_REVOKED: []const u8 = "UZ-RUN-009";
pub const ERR_RUN_LEASE_EXCEEDED_MAX_RUNTIME: []const u8 = "UZ-RUN-010";
pub const ERR_RUN_LEASE_LOST: []const u8 = "UZ-RUN-011";
pub const ERR_RUN_LEASE_RENEWAL_NO_CREDITS: []const u8 = "UZ-RUN-012";
