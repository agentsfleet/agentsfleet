//! Tests for the engine DI seam (run_context.zig + executeInner wiring, M100).
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
