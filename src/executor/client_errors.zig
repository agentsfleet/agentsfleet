//! ExecutorClient error set + RPC-error-code → FailureClass mapping.
//! Split out of client.zig per RULE FLL — pure mapping, no state.

const protocol = @import("protocol.zig");
const types = @import("types.zig");

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

/// Maps an RPC error code to a FailureClass.
pub fn classifyError(code: i32) types.FailureClass {
    return switch (code) {
        protocol.ErrorCode.timeout_killed => .timeout_kill,
        protocol.ErrorCode.oom_killed => .oom_kill,
        protocol.ErrorCode.policy_denied => .policy_deny,
        protocol.ErrorCode.lease_expired => .lease_expired,
        protocol.ErrorCode.landlock_denied => .landlock_deny,
        protocol.ErrorCode.resource_killed => .resource_kill,
        else => .executor_crash,
    };
}

// Error-code mirrors of src/errors/error_registry.zig — the executor binary
// tree forbids imports outside src/executor/ so the canonical strings are
// duplicated here. Exposed as `pub` for client.zig's RPC-error log paths.
pub const ERR_EXEC_TIMEOUT_KILL: []const u8 = "UZ-EXEC-003";
pub const ERR_EXEC_OOM_KILL: []const u8 = "UZ-EXEC-004";
pub const ERR_EXEC_RESOURCE_KILL: []const u8 = "UZ-EXEC-005";
pub const ERR_EXEC_TRANSPORT_LOSS: []const u8 = "UZ-EXEC-006";
pub const ERR_EXEC_LEASE_EXPIRED: []const u8 = "UZ-EXEC-007";
pub const ERR_EXEC_CRASH: []const u8 = "UZ-EXEC-010";
pub const ERR_EXEC_LANDLOCK_DENY: []const u8 = "UZ-EXEC-011";

/// Maps an RPC error code to its canonical UZ-EXEC-NNN string for the
/// `error_code` log field. Lets operators page on the actual failure
/// class (timeout, oom, policy, etc.) instead of UZ-EXEC-006 catching
/// every executor-rejected request as transport loss.
///
/// Reserved: `ERR_EXEC_TRANSPORT_LOSS` (UZ-EXEC-006) is for the
/// catch-arm where `sendRequest*` itself fails — the wire transport
/// dropped the frame. Application-level RPC errors land at the rpc_error
/// arm and route through this function instead.
pub fn errorCodeForRpcCode(code: i32) []const u8 {
    return switch (code) {
        protocol.ErrorCode.timeout_killed => ERR_EXEC_TIMEOUT_KILL,
        protocol.ErrorCode.oom_killed => ERR_EXEC_OOM_KILL,
        protocol.ErrorCode.resource_killed => ERR_EXEC_RESOURCE_KILL,
        protocol.ErrorCode.lease_expired => ERR_EXEC_LEASE_EXPIRED,
        protocol.ErrorCode.landlock_denied => ERR_EXEC_LANDLOCK_DENY,
        // policy_denied + execution_failed + JSON-RPC envelope errors
        // (parse_error, invalid_request, etc.) all collapse to the
        // executor-crash bucket — the registry lacks a UZ-EXEC code for
        // policy denial today; ERR_EXEC_CRASH is the closest fit.
        else => ERR_EXEC_CRASH,
    };
}
