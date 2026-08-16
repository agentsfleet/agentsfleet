//! Tests for the engine dependency-injection seam (run_context.zig +
//! executeInner wiring).
//! Proves the production execute path acquires its LLM provider through the
//! injectable `RunDeps.acquireProvider` — the seam that makes the engine path
//! drivable offline (previously the provider was hard-wired and untestable).

const std = @import("std");
const nullclaw = @import("nullclaw");
const providers = nullclaw.providers;
const Config = nullclaw.config.Config;

const run_context = @import("run_context.zig");
const runner = @import("runner.zig");
const runner_helpers = @import("runner_helpers.zig");
const wire = @import("wire.zig");
const pipe_proto = @import("../pipe_proto.zig");
const clock = @import("common").clock;

/// The drain below reads a pipe the run already finished writing, so the
/// deadline only exists to stop a regression from hanging the lane.
const FRAME_DRAIN_DEADLINE_MS: i64 = 5_000;
const MAX_FRAME_PAYLOAD: usize = 1 << 20;

test "RunDeps default wires the runtime provider acquirer" {
    const deps = run_context.RunDeps{};
    try std.testing.expect(deps.acquireProvider == run_context.runtimeAcquireProvider);
}

// File-scoped invocation counter — a `*const fn` can't capture state, so the
// stub records through this module global (reset per test).
var stub_invocations: usize = 0;

fn stubAcquireFail(
    _: std.mem.Allocator,
    _: *Config,
    _: *runner_helpers.ProviderBundle,
) anyerror!providers.Provider {
    stub_invocations += 1;
    return error.StubProviderInjected;
}

test "executeInner acquires its provider through the injected seam, offline" {
    const alloc = std.testing.allocator;
    stub_invocations = 0;

    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();

    const deps = run_context.RunDeps{ .acquireProvider = stubAcquireFail };
    const result = runner.executeInner(
        deps,
        &env_map,
        alloc,
        "/tmp/agentsfleet-runctx-test",
        null, // fleet_config
        null, // tools_spec
        "hello",
        null, // context
        null, // policy
        null, // progress_fd
        &.{}, // hydrated_memory
    );

    // The injected acquirer was reached exactly once (proving step 1 — config
    // load + overrides — runs offline and the provider step routes through the
    // seam) and its failure propagated as FleetInitFailed.
    try std.testing.expectEqual(@as(usize, 1), stub_invocations);
    try std.testing.expectError(runner.RunnerError.FleetInitFailed, result);
}

// A provider that acquires cleanly but rejects the model call, so `executeInner`
// reaches `fleet.runSingle` and fails THERE rather than at provider acquisition.
// That is the only way to exercise the run-failure path offline.
const REJECTED = error.StubChatRejected;

fn stubChatWithSystem(_: *anyopaque, _: std.mem.Allocator, _: ?[]const u8, _: []const u8, _: []const u8, _: f64) anyerror![]const u8 {
    return REJECTED;
}

fn stubChat(_: *anyopaque, _: std.mem.Allocator, _: providers.ChatRequest, _: []const u8, _: f64) anyerror!providers.ChatResponse {
    return REJECTED;
}

fn stubSupportsNativeTools(_: *anyopaque) bool {
    return false;
}

fn stubGetName(_: *anyopaque) []const u8 {
    return "stub";
}

fn stubProviderDeinit(_: *anyopaque) void {}

const stub_vtable = providers.Provider.VTable{
    .chatWithSystem = stubChatWithSystem,
    .chat = stubChat,
    .supportsNativeTools = stubSupportsNativeTools,
    .getName = stubGetName,
    .deinit = stubProviderDeinit,
};

var stub_provider_state: u8 = 0;

fn stubAcquireRejectingProvider(
    _: std.mem.Allocator,
    _: *Config,
    _: *runner_helpers.ProviderBundle,
) anyerror!providers.Provider {
    return .{ .ptr = @ptrCast(&stub_provider_state), .vtable = &stub_vtable };
}

// A model must be configured or `Fleet.fromConfig` short-circuits on
// `NoDefaultModel` and the run never reaches the provider call this test is about.
const STUB_FLEET_CONFIG = std.fmt.comptimePrint(
    "{{\"{s}\":\"stub-model\",\"{s}\":\"stub\"}}",
    .{ wire.model, wire.provider },
);

test "a run failure propagates its own error instead of collapsing to FleetRunFailed" {
    const alloc = std.testing.allocator;

    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, STUB_FLEET_CONFIG, .{});
    defer parsed.deinit();

    const deps = run_context.RunDeps{ .acquireProvider = stubAcquireRejectingProvider };
    const result = runner.executeInner(
        deps,
        &env_map,
        alloc,
        "/tmp/agentsfleet-runctx-runfail-test",
        parsed.value, // fleet_config — carries the model
        null, // tools_spec
        "hello",
        null, // context
        null, // policy
        null, // progress_fd
        &.{}, // hydrated_memory
    );

    // `FleetRunFailed` is the outcome LABEL. When the run-failure sites collapse to
    // it, `execute`'s `.detail = @errorName(err)` renders the tautology
    // "FleetRunFailed" and the operator learns nothing from the failure detail. The
    // error that actually stopped the run must survive to that detail — asserted as
    // "not the label" rather than one exact error so the provider stack stays free
    // to wrap the stub's rejection.
    if (result) |_| {
        return error.TestExpectedRunFailure;
    } else |err| {
        try std.testing.expect(err != runner.RunnerError.FleetRunFailed);
    }
}

// ---------------------------------------------------------------------------
// The success tail. Every stub above rejects, so `executeInner` had never
// returned normally under any test: the redacted-reply hand-off, the usage
// split and the return literal were dark, and so was every progress-fd wiring
// line — the stream callback, the memory capturer, the terminal usage frame.
// A provider that ANSWERS drives all of it offline, in-process.
// ---------------------------------------------------------------------------

const STUB_REPLY = "stub reply";
const STUB_PROMPT_TOKENS: u32 = 11;
const STUB_COMPLETION_TOKENS: u32 = 7;

fn stubChatWithSystemOk(_: *anyopaque, alloc: std.mem.Allocator, _: ?[]const u8, _: []const u8, _: []const u8, _: f64) anyerror![]const u8 {
    return alloc.dupe(u8, STUB_REPLY);
}

fn stubChatOk(_: *anyopaque, alloc: std.mem.Allocator, _: providers.ChatRequest, _: []const u8, _: f64) anyerror!providers.ChatResponse {
    return .{
        // Owned by the caller: `ChatResponse.deinit` frees content with this
        // same allocator, so a literal here would be a free of static memory.
        .content = try alloc.dupe(u8, STUB_REPLY),
        .usage = .{
            .prompt_tokens = STUB_PROMPT_TOKENS,
            .completion_tokens = STUB_COMPLETION_TOKENS,
            .total_tokens = STUB_PROMPT_TOKENS + STUB_COMPLETION_TOKENS,
        },
    };
}

const answering_vtable = providers.Provider.VTable{
    .chatWithSystem = stubChatWithSystemOk,
    .chat = stubChatOk,
    .supportsNativeTools = stubSupportsNativeTools,
    .getName = stubGetName,
    .deinit = stubProviderDeinit,
};

fn stubAcquireAnsweringProvider(
    _: std.mem.Allocator,
    _: *Config,
    _: *runner_helpers.ProviderBundle,
) anyerror!providers.Provider {
    return .{ .ptr = @ptrCast(&stub_provider_state), .vtable = &answering_vtable };
}

/// One clean run against the answering provider. The caller owns `.content`.
fn runToCompletion(alloc: std.mem.Allocator, progress_fd: ?std.posix.fd_t) !runner.InnerResult {
    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, STUB_FLEET_CONFIG, .{});
    defer parsed.deinit();

    return runner.executeInner(
        .{ .acquireProvider = stubAcquireAnsweringProvider },
        &env_map,
        alloc,
        "/tmp/agentsfleet-runctx-success-test",
        parsed.value, // carries the model, or Fleet.fromConfig short-circuits
        null, // tools_spec
        "hello",
        null, // context
        null, // policy
        progress_fd,
        &.{}, // hydrated_memory
    );
}

test "a clean run returns the reply and maps the usage split at the engine boundary" {
    const alloc = std.testing.allocator;
    const result = try runToCompletion(alloc, null);
    defer alloc.free(result.content);

    try std.testing.expectEqualStrings(STUB_REPLY, result.content);
    // The split is the billing-facing claim: prompt-side tokens must land on
    // `input`, completion-side on `output`. A transposition here bills every
    // run against the wrong side and no other test would catch it.
    try std.testing.expectEqual(@as(u64, STUB_PROMPT_TOKENS), result.input_tokens);
    try std.testing.expectEqual(@as(u64, STUB_COMPLETION_TOKENS), result.output_tokens);
}

test "a progress fd carries a terminal usage frame off the same clean run" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    // Closed by hand once the run is done so the drain below reaches a clean
    // EOF; the flag keeps the failure path from closing it twice.
    var writer_open = true;
    defer if (writer_open) pipe_proto.testOsClose(fds[1]);

    const result = try runToCompletion(alloc, fds[1]);
    defer alloc.free(result.content);
    pipe_proto.testOsClose(fds[1]);
    writer_open = false;

    var saw_usage = false;
    var frames: usize = 0;
    const deadline = clock.nowMillis() + FRAME_DRAIN_DEADLINE_MS;
    while (true) {
        switch (try pipe_proto.readFrame(alloc, fds[0], deadline, MAX_FRAME_PAYLOAD)) {
            .eof, .timed_out => break,
            .frame => |f| {
                defer alloc.free(f.payload);
                frames += 1;
                if (f.ftype == .usage) saw_usage = true;
            },
        }
    }

    // With a progress fd the adapter is the fleet's observer AND its per-token
    // stream callback, and it emits one terminal usage frame after the last
    // token fold. A run that produced the reply but no usage frame bills the
    // lease off whatever the last mid-run checkpoint happened to carry.
    try std.testing.expect(frames > 0);
    try std.testing.expect(saw_usage);
    try std.testing.expectEqualStrings(STUB_REPLY, result.content);
}
