// Wire-level redaction tests. Spawns `zombied-executor-stub` (production
// NullClaw pipeline + observer + redactor adapter all live, only the LLM
// provider is swapped for the canned-response stub), drives a StartStage
// with `agent_config.api_key = SYNTHETIC_SECRET`, and asserts on the
// bytes the worker reads off the executor RPC socket:
//
//   - the placeholder `${secrets.llm.api_key}` appears in the captured
//     stream (redactor substituted as expected);
//   - the resolved secret bytes never appear (no leak through tool_use
//     args, agent_response_chunk content, terminal StageResponse, or any
//     other frame the executor emitted during the run).
//
// The stub provider's canned response carries the synthetic secret in
// BOTH `content` (textDelta path) AND tool_call `arguments` (tool_use
// observer path), so a single run exercises both redactor branches.

const std = @import("std");
const Allocator = std.mem.Allocator;

const canary = @import("../executor/redaction_canary.zig");
const Harness = @import("test_executor_harness.zig");
const RpcRecorder = @import("test_rpc_recorder.zig");

const ALLOC = std.testing.allocator;

const SYNTHETIC_SECRET = canary.SYNTHETIC_SECRET;
const PLACEHOLDER = canary.PLACEHOLDER;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f40";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
const TEST_TRACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa42";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa41";

const RedactionRun = struct {
    harness: Harness,
    recorder: RpcRecorder,

    fn deinit(self: *RedactionRun) void {
        self.recorder.uninstall();
        self.recorder.deinit();
        self.harness.deinit();
    }
};

fn driveRedactionRun(alloc: Allocator) !RedactionRun {
    var harness = try Harness.start(alloc, .{ .binary = .stub });
    errdefer harness.deinit();

    var recorder = RpcRecorder.init(alloc);
    errdefer recorder.deinit();
    recorder.install();
    errdefer recorder.uninstall();

    const execution_id = try harness.executor.createExecution(.{
        .workspace_path = "/tmp",
        .correlation = .{
            .trace_id = TEST_TRACE_ID,
            .zombie_id = TEST_ZOMBIE_ID,
            .workspace_id = TEST_WORKSPACE_ID,
            .session_id = TEST_SESSION_ID,
        },
    });
    defer alloc.free(execution_id);
    defer harness.executor.destroyExecution(execution_id) catch {};

    // The stub provider's canned response will fire tool_use + chunk
    // observer events containing SYNTHETIC_SECRET. Tool dispatch may
    // fail (no matching tool in the empty tools list) — we don't care:
    // the redactor runs BEFORE dispatch, so the bytes on the wire are
    // already the assertion target.
    if (harness.executor.startStage(execution_id, .{
        .agent_config = .{
            .model = "stub",
            .provider = "stub",
            .api_key = SYNTHETIC_SECRET,
        },
        .message = "redact me",
    })) |result| {
        alloc.free(result.content);
        if (result.checkpoint_id) |c| alloc.free(c);
    } else |_| {}

    return .{ .harness = harness, .recorder = recorder };
}

test "test_executor_args_redacted_at_sandbox_boundary" {
    var run = try driveRedactionRun(ALLOC);
    defer run.deinit();

    try std.testing.expect(run.recorder.contains(PLACEHOLDER));
    try std.testing.expect(!run.recorder.contains(SYNTHETIC_SECRET));
}

test "test_executor_passes_through_redacted_for_chunks" {
    // Same canned run as the boundary test — the stub's `content` field
    // (the textDelta-path payload) carries SYNTHETIC_SECRET literally,
    // so the chunk-redaction branch fires alongside the tool-arg branch
    // in a single run. This block isolates the chunk-side claim: no
    // agent_response_chunk frame leaks the secret.
    var run = try driveRedactionRun(ALLOC);
    defer run.deinit();

    try std.testing.expect(!run.recorder.contains(SYNTHETIC_SECRET));
}

// `test_args_redacted_no_secret_leak` (pub/sub-side assertion) is tracked
// separately; it requires the worker→Redis publish path which depends on
// a live Postgres + Redis test environment. Captured here as a plain
// reminder rather than skipped, to keep this file integration-free.
//   See M42_002 spec §Test Specification, row 2.
