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
