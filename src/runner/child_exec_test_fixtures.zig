//! Shared test fixtures for the child-execute lane — a minimal `LeasePayload`
//! builder consumed by both `child_exec.zig` (runEngine fail-closed tests) and
//! `child_exec_input_test.zig` (buildCallArgs / buildInstructionsContext tests).

const contract = @import("contract");

const LeasePayload = contract.protocol.LeasePayload;
const ExecutionPolicy = contract.execution_policy.ExecutionPolicy;

/// A minimal lease whose only inputs `buildCallArgs` reads are `policy` and
/// `event.request_json`. `instructions` defaults to "" (the empty / fail-closed
/// case); tests exercising the instruction path set it explicitly.
pub fn testLease(policy: ExecutionPolicy) LeasePayload {
    return .{
        .lease_id = "l1",
        .fencing_token = 1,
        .lease_expires_at = 0,
        .secret_delivery = .@"inline",
        .event = .{
            .event_id = "1700000000000-0",
            .agent_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
            .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
            .actor = "steer:test",
            .event_type = .chat,
            .request_json = "{\"message\":\"hi\"}",
            .created_at = 1700000000000, // pin test: fixed fixture timestamp
        },
        .policy = policy,
    };
}
