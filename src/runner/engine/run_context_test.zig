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
