//! Test-only harness path for the executor runner.
//!
//! Activated at build time by `-Dexecutor_harness=true`, which produces the
//! `zombied-executor-harness` binary. The production binary
//! (`zombied-executor`) is built with the flag false, so the comptime
//! branch in `runner.execute` is stripped and none of this code ships.
//!
//! Purpose: integration tests that need deterministic frame emission and
//! synthetic completion without burning tokens on a real LLM. The harness
//! reads a script from `EXECUTOR_HARNESS_SCRIPT` (absolute path to a JSON
//! file), emits the listed progress frames via the same `progress_writer`
//! the production runner uses, and returns a synthetic ExecutionResult.
//!
//! Phase 1 (this commit): skeleton that returns success; script parsing
//! and frame emission land with the dependent tests.

const std = @import("std");
const types = @import("types.zig");
const progress_writer_mod = @import("progress_writer.zig");

const log = std.log.scoped(.executor_runner_harness);

/// Mirrors `runner.execute`'s signature so the comptime branch is a
/// drop-in replacement. Returns a synthetic ExecutionResult; future
/// commits will read `EXECUTOR_HARNESS_SCRIPT` and emit scripted frames
/// via `progress` before returning.
pub fn execute(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    agent_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: ?[]const u8,
    context: ?std.json.Value,
    progress: ?*const progress_writer_mod,
) types.ExecutionResult {
    _ = alloc;
    _ = workspace_path;
    _ = agent_config;
    _ = tools_spec;
    _ = context;
    _ = progress;

    log.info("harness.execute message_len={d}", .{if (message) |m| m.len else 0});

    return .{
        .content = "",
        .exit_ok = true,
        .failure = null,
    };
}
