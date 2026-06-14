//! Engine error-code string constants.

// Error-code mirrors of src/agentsfleetd/errors/error_registry.zig — the runner
// binary tree forbids imports into src/agentsfleetd/ (build_runner.zig keeps the
// runner portable) so the canonical strings are duplicated here. Every runner
// source needing a UZ-EXEC-* / UZ-TOOL-* / UZ-RUN-* literal MUST import from
// this file — never declare a local `const ERR_X` in another runner source.
// `audit-error-codes.sh --strict` flags raw `"UZ-…"` literals outside this file.
pub const ERR_EXEC_TIMEOUT_KILL: []const u8 = "UZ-EXEC-003";
pub const ERR_EXEC_OOM_KILL: []const u8 = "UZ-EXEC-004";
pub const ERR_EXEC_RUNNER_AGENT_INIT: []const u8 = "UZ-EXEC-012";
pub const ERR_EXEC_RUNNER_AGENT_RUN: []const u8 = "UZ-EXEC-013";
pub const ERR_EXEC_RUNNER_INVALID_CONFIG: []const u8 = "UZ-EXEC-014";
pub const ERR_TOOL_UNKNOWN: []const u8 = "UZ-TOOL-005";

// Fleet control-plane code the parent supervisor emits when a lease's mandatory
// sandbox cannot be established and the lease is refused unrun (Invariant 7).
pub const ERR_RUN_SANDBOX_ESTABLISH_FAILED: []const u8 = "UZ-RUN-007";

