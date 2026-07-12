//! Engine error-code string constants.

const types = @import("types.zig");

// Error-code mirrors of src/agentsfleetd/errors/error_registry.zig — the runner
// binary tree forbids imports into src/agentsfleetd/ (build_runner.zig keeps the
// runner portable) so the canonical strings are duplicated here. Every runner
// source needing a UZ-EXEC-* / UZ-TOOL-* / UZ-RUN-* literal MUST import from
// this file — never declare a local `const ERR_X` in another runner source.
// `audit-error-codes.sh --strict` flags raw `"UZ-…"` literals outside this file.
pub const ERR_EXEC_TIMEOUT_KILL: []const u8 = "UZ-EXEC-003";
pub const ERR_EXEC_OOM_KILL: []const u8 = "UZ-EXEC-004";
pub const ERR_EXEC_RESOURCE_KILL: []const u8 = "UZ-EXEC-005";
pub const ERR_EXEC_TRANSPORT_LOSS: []const u8 = "UZ-EXEC-006";
pub const ERR_EXEC_LEASE_EXPIRED: []const u8 = "UZ-EXEC-007";
pub const ERR_EXEC_RENEWAL_TERMINATED: []const u8 = "UZ-EXEC-008";
pub const ERR_EXEC_STARTUP_POSTURE: []const u8 = "UZ-EXEC-009";
pub const ERR_EXEC_CRASH: []const u8 = "UZ-EXEC-010";
pub const ERR_EXEC_LANDLOCK_DENY: []const u8 = "UZ-EXEC-011";
pub const ERR_EXEC_RUNNER_FLEET_INIT: []const u8 = "UZ-EXEC-012";
pub const ERR_EXEC_RUNNER_FLEET_RUN: []const u8 = "UZ-EXEC-013";
pub const ERR_EXEC_RUNNER_INVALID_CONFIG: []const u8 = "UZ-EXEC-014";
pub const ERR_EXEC_BUDGET_BREACH: []const u8 = "UZ-EXEC-015";
pub const ERR_EXEC_RUNNER_TOKEN_REJECTED: []const u8 = "UZ-EXEC-016";
pub const ERR_TOOL_UNKNOWN: []const u8 = "UZ-TOOL-005";

/// Control-plane code the `/renew` refusal carries when a fleet has reached its
/// own `daily_dollars`/`monthly_dollars` ceiling. The runner reads it off the
/// 402 body to tell a budget stop from a credit stop (UZ-RUN-012).
pub const ERR_RUN_BUDGET_EXCEEDED: []const u8 = "UZ-RUN-015";

// Fleet control-plane code the parent supervisor emits when a lease's mandatory
// sandbox cannot be established and the lease is refused unrun (Invariant 7).
pub const ERR_RUN_SANDBOX_ESTABLISH_FAILED: []const u8 = "UZ-RUN-007";

/// Canonical `FailureClass` -> `UZ-EXEC-*` code (exhaustive: no `else`, so a new
/// variant must add an arm). The durable surface is `failure_label`; this is the
/// log / API-error annotation, kept 1:1 with the class by name.
pub fn errorCodeForFailure(failure: types.FailureClass) []const u8 {
    return switch (failure) {
        .startup_posture => ERR_EXEC_STARTUP_POSTURE,
        .policy_deny => ERR_EXEC_RUNNER_FLEET_RUN, // latent: no emit site yet
        .timeout_kill => ERR_EXEC_TIMEOUT_KILL,
        .oom_kill => ERR_EXEC_OOM_KILL,
        .resource_kill => ERR_EXEC_RESOURCE_KILL,
        .runner_crash => ERR_EXEC_CRASH,
        .transport_loss => ERR_EXEC_TRANSPORT_LOSS,
        .landlock_deny => ERR_EXEC_LANDLOCK_DENY,
        .lease_expired => ERR_EXEC_LEASE_EXPIRED,
        .renewal_terminate => ERR_EXEC_RENEWAL_TERMINATED,
        .budget_breach => ERR_EXEC_BUDGET_BREACH,
    };
}
